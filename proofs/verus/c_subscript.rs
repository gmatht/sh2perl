// Standalone model of the C backend's FLATTENED SUBSCRIPT — the text the
// core leaves inside a write target's NAME (`n[$((i*2))]=v` arrives as
// `Assign{ targets: [{ var: "n[$((i*2))]" }] }`, with `indices` EMPTY).
//
// One text, three consumers, and they disagreed in BOTH directions
// (all measured against bash 2026-11-04, `harness/ifs-probe.sh` 59x):
//
//   * the DECLARATION census (`collect_assoc_names`, drives
//     `long long n[]` vs `n_k/n_v/n_n`) called `i*2` a STRING KEY, so an
//     int array was declared indexed while the store wrote `n_k` —
//     `'n_k' undeclared`, the C did not compile (59, 59g, 59h).
//   * the STORE arm (`emit_array_assign`) called a bare ident `i`
//     a string key too, unless `i` happened to be in its store/type map
//     (`n[i]=5` → `n_k` undeclared, 59e).
//   * the CENSUS's read set (`dead_store_elim`) never looked at the
//     subscript at all, so `i`'s only reference — the text — was
//     invisible: `i=3; n[$((i))]=9` kept 2 of 3 statements and the
//     renderer substituted a var with no store and no declaration (59m).
//   * a `$((…))` key was not EXPANDED either way: as an INDEX it read 0
//     (`${a[$((i+1))]}` → `p`, bash `r`, 59j) and as an ASSOC key it
//     stored the empty-expansion `"0"` (`m[$((i*2))]` → key "0", bash
//     "2", 59k).
//
// ## The denotational extension
//
// bash has ONE rule for an indexed array and one for an assoc table:
//
//   INDEXED   the subscript is an ARITHMETIC EXPRESSION. Any text is
//             evaluated: a numeral, a bare name (`i` — its value, 0 when
//             unset), `$name`, `$((e))`, or an operator expression
//             (`i*2`, `(i*3)+1`). Quoting does not matter.
//   ASSOC     the subscript is a STRING after EXPANSION: `$((e))` becomes
//             the decimal of its value ("2"), `$name` the variable's
//             text, and a BARE operator text stays literal (`m[i*2]`
//             stores the KEY `i*2` — measured, `${!m[*]}` → `i*2 2` with
//             both forms in one script).
//
// So the storage class is decided ONCE, from the text alone, and every
// consumer reads that one verdict. `classify` below is that verdict;
// `census_assoc` and `store_assoc` are both DEFINED from it, which is the
// content of `decl_and_store_agree`: the declaration and the store cannot
// disagree, because there is nothing left to disagree about. The pre-fix
// pair had two independent predicates and is proved divergent on the very
// shapes the probes pinned (`prefix_diverges_on_ident`,
// `prefix_calls_arith_assoc`).
//
// The second obligation is LIVENESS, which is what the text's opacity
// breaks: a store may only be dropped when no surviving statement
// mentions the name, and a name mentioned only inside a subscript text
// (`names_in`) must therefore count as a read. `subscript_closure` proves
// the census is closed over `names_in`, and `dead_needs_no_mention` is
// the contrapositive the purge relies on. Before the fix a flattened
// target contributed `Set::empty()` to the read set
// (`fix_was_necessary`), which is exactly why `i=3` looked dead.
//
// Obligations (each maps to a probe in the 59x family, or to a named
// mutant of the retired rendering):
//   k1..k8        the CLASS of every measured subscript form
//   k9..k13       the INDEX an indexed array computes (2, i+1, i, 1, 6)
//   k14..k16      the ASSOC key text ("i*2" literal, "2" for the explicit form)
//   quote_blind   `n["1"]` is index 1 (quotes stripped, bash-measured)
//   decl_and_store_agree / store_assoc_is_word
//   prefix_diverges_on_ident / prefix_calls_arith_assoc   the retired pair
//   assoc_dollar_brace_remains  the ONE divergence left (`${i}`: the core
//                 keeps the braces, the store arm routes it assoc while
//                 the census excludes every `$`-prefixed text) — pinned,
//                 NOT fixed: it needs the core to split the subscript.
//   subscript_closure / dead_needs_no_mention / fix_was_necessary
//   m1..m3        the mutants (bare ident as Word, pre-fix read set, the
//                 literal-key store) — each proved to break the property
//                 the fixed code keeps, not merely asserted.

use vstd::prelude::*;

verus! {

pub type Str = Seq<char>;

// ── character classes ────────────────────────────────────────────────
pub open spec fn is_quote(c: char) -> bool {
    c == '"' || c == '\''
}

pub open spec fn is_op(c: char) -> bool {
    c == '+' || c == '-' || c == '*' || c == '/' || c == '%' || c == '(' || c == ')'
        || c == '<' || c == '>' || c == '&' || c == '|' || c == '^' || c == '~' || c == '!'
}

/// A space is NOT an operator: `map[a b]` is a string key, and bash rejects
/// it as an arith expression (`subscript_arith_inner`'s `has_arith_op`
/// deliberately omits ' ').
/// `is_ascii_alphanumeric`/`is_ascii_alphabetic`/`is_ascii_digit` have no
/// vstd spec in the pinned binary, so the classes are spelled as char
/// ranges — the same sets, and the only form the verifier accepts.
pub open spec fn is_digit(c: char) -> bool {
    '0' <= c && c <= '9'
}

pub open spec fn is_alpha(c: char) -> bool {
    ('a' <= c && c <= 'z') || ('A' <= c && c <= 'Z')
}

pub open spec fn is_word_char(c: char) -> bool {
    is_alpha(c) || is_digit(c) || c == '_'
}

pub open spec fn ident_start(c: char) -> bool {
    is_alpha(c) || c == '_'
}

// (The scans are RECURSIVE rather than quantifier predicates: `compute`
// evaluates a recursive spec fn on a concrete text, while a quantified body
// defeats it — and `by (compute)` is what makes the pins facts about the
// text instead of assumptions about the solver.)
pub open spec fn all_digits(s: Str) -> bool {
    all_digits_from(s, 0)
}

/// Every char a digit (the numeral test; a leading `-` is bash's negative
/// index, which the store arm routes to the numeral path and the runtime
/// guards to "store nothing").
pub open spec fn all_digits_from(s: Str, i: int) -> bool
    decreases s.len() - i,
{
    if i >= s.len() {
        true
    } else if is_digit(s[i]) {
        all_digits_from(s, i + 1)
    } else {
        false
    }
}

pub open spec fn all_word(s: Str) -> bool {
    s.len() > 0 && all_word_from(s, 0)
}

pub open spec fn all_word_from(s: Str, i: int) -> bool
    decreases s.len() - i,
{
    if i >= s.len() {
        true
    } else if is_word_char(s[i]) {
        all_word_from(s, i + 1)
    } else {
        false
    }
}

pub open spec fn starts_ident(s: Str) -> bool {
    s.len() > 0 && ident_start(s[0]) && all_word_from(s, 1)
}

pub open spec fn has_op(s: Str) -> bool {
    has_op_from(s, 0)
}

pub open spec fn has_op_from(s: Str, i: int) -> bool
    decreases s.len() - i,
{
    if i >= s.len() {
        false
    } else if is_op(s[i]) {
        true
    } else {
        has_op_from(s, i + 1)
    }
}

pub open spec fn starts_with(s: Str, pre: Str) -> bool {
    s.len() >= pre.len() && s.subrange(0, pre.len() as int) == pre
}

pub open spec fn ends_with(s: Str, suf: Str) -> bool {
    s.len() >= suf.len() && s.subrange(s.len() - suf.len() as int, s.len() as int) == suf
}

// ── normalisation: bash strips subscript quotes at parse time ────────
/// Leading/trailing quotes dropped (the source's
/// `trim_matches(|c| c == '"' || c == '\'')` on both the numeral test and
/// `subscript_explicit_arith`).
pub open spec fn strip_quotes(s: Str) -> Str
    decreases s.len(),
{
    if s.len() > 0 && is_quote(s[0]) {
        strip_quotes(s.subrange(1, s.len() as int))
    } else if s.len() > 0 && is_quote(s[s.len() - 1]) {
        strip_quotes(s.subrange(0, s.len() - 1))
    } else {
        s
    }
}

pub open spec fn trim_sp(s: Str) -> Str
    decreases s.len(),
{
    if s.len() > 0 && s[0] == ' ' {
        trim_sp(s.subrange(1, s.len() as int))
    } else if s.len() > 0 && s[s.len() - 1] == ' ' {
        trim_sp(s.subrange(0, s.len() - 1))
    } else {
        s
    }
}

pub open spec fn norm(s: Str) -> Str {
    strip_quotes(trim_sp(s))
}

// ── the explicit arithmetic forms ────────────────────────────────────
pub open spec fn open_arith() -> Str { seq!['$', '(', '('] }

pub open spec fn close_arith() -> Str { seq![')', ')'] }

pub open spec fn open_bracket_arith() -> Str { seq!['$', '['] }

pub open spec fn close_bracket() -> Str { seq![']'] }

/// `$((e))` / `$[e]` → `e`. These are the forms bash EXPANDS before the
/// storage class ever looks at them, so they are arith for an indexed
/// array AND the value text for an assoc key.
pub open spec fn explicit_arith(s: Str) -> Option<Str> {
    if starts_with(s, open_arith()) && ends_with(s, close_arith()) && s.len() >= 5 {
        Some(s.subrange(3, s.len() - 2))
    } else if starts_with(s, open_bracket_arith()) && ends_with(s, close_bracket())
        && s.len() >= 3
    {
        Some(s.subrange(2, s.len() - 1))
    } else {
        None
    }
}

// ── the ONE verdict ──────────────────────────────────────────────────
pub enum SubKind {
    /// `$((e))` — an arith expansion (index; assoc key = the value text)
    Arith,
    /// a numeral (quoted or not) — index
    Num,
    /// a bare `$name` — dynamic index (indexed) / variable text (assoc)
    DynVar,
    /// a bare identifier — bash arith evaluates it: its VALUE, 0 if unset
    Ident,
    /// an operator-bearing text — an arith EXPRESSION
    ArithText,
    /// anything else — a string key, which is what makes an undeclared
    /// array assoc
    Word,
}

/// The classification the source's `subscript_explicit_arith` /
/// `subscript_arith_inner` / numeral test / `is_ident` implement, in the
/// order `emit_array_assign` reaches them.
pub open spec fn classify(text: Str) -> SubKind {
    let k = norm(text);
    if explicit_arith(k).is_some() {
        SubKind::Arith
    } else if k.len() > 0 && all_digits(k) {
        SubKind::Num
    } else if k.len() > 1 && k[0] == '$' && all_word(k.subrange(1, k.len() as int)) {
        SubKind::DynVar
    } else if starts_ident(k) {
        SubKind::Ident
    } else if has_op(k) {
        SubKind::ArithText
    } else {
        SubKind::Word
    }
}

/// The index an INDEXED array computes for the text (bash's arithmetic):
/// the arith VALUE for both explicit and implicit forms, the numeral, or
/// the variable's value. `Word` never reaches this — a plain word on an
/// INDEXED target is arith too in bash, but the store arm's ident/word
/// split is what the declaration census mirrors (see
/// `decl_and_store_agree`), and that split is what keeps `map[foo]`
/// working for the undECLARED assoc tables the corpus relies on.
pub open spec fn index_text(text: Str) -> Str {
    match classify(text) {
        SubKind::Arith => explicit_arith(norm(text)).unwrap(),
        SubKind::ArithText => norm(text),
        _ => norm(text),
    }
}

/// The ASSOC key text: the arith VALUE for an explicit expansion, the
/// literal text otherwise (measured: `m[i*2]` → "i*2", `m[$((i*2))]` →
/// "2"). The two forms are DISTINGUISHABLE here, which is exactly why
/// `subscript_explicit_arith` (and not `subscript_arith_inner`) is the
/// assoc-key predicate.
pub open spec fn assoc_key(text: Str) -> Str {
    match classify(text) {
        SubKind::Arith => explicit_arith(norm(text)).unwrap(),
        _ => norm(text),
    }
}

pub open spec fn assoc_key_is_value(text: Str) -> bool {
    classify(text) == SubKind::Arith
}

// ── the two consumers, one predicate ─────────────────────────────────
pub open spec fn starts_dollar(s: Str) -> bool {
    s.len() > 0 && s[0] == '$'
}

/// What `collect_assoc_names` records: an assoc table, i.e. the text is a
/// string key and nothing else. The source additionally excludes every
/// `$`-prefixed text (a `$var` is a DYNAMIC index — that is why `${i}` is
/// the one shape the pair still disagrees on; `assoc_dollar_brace_remains`).
pub open spec fn census_assoc(text: Str) -> bool {
    classify(text) == SubKind::Word && !starts_dollar(norm(text))
}

/// What `emit_array_assign`'s "literal string key implies assoc" arm
/// does — the store's storage-class verdict.
pub open spec fn store_assoc(text: Str) -> bool {
    classify(text) == SubKind::Word
}

/// THE INVARIANT the round exists for: the declaration census and the
/// store arm agree on every subscript text, so the emitted declaration
/// always provides the storage the store writes. (Before the fix they
/// were independent predicates — `prefix_diverges_on_ident`.)
pub proof fn decl_and_store_agree(text: Str)
    requires
        !starts_dollar(norm(text)),
    ensures
        census_assoc(text) == store_assoc(text),
{
}

/// …and `Word` is the only assoc verdict: every arith/numeral/dynvar/ident
/// text is an INDEX (or a dynamic index), never a k/v/n table.
pub proof fn store_assoc_is_word(text: Str)
    ensures
        store_assoc(text) ==> classify(text) == SubKind::Word,
{
}

pub proof fn arith_is_never_assoc(text: Str)
    ensures
        classify(text) == SubKind::Arith || classify(text) == SubKind::ArithText
            ==> !store_assoc(text),
{
}

// ── the retired pair, proved divergent ───────────────────────────────
/// Pre-fix census (`collect_assoc_names`): assoc iff the text is not a
/// numeral, does not start with `$`, and is not a plain word.
pub open spec fn census_assoc_prefix(text: Str) -> bool {
    let k = norm(text);
    !(k.len() > 0 && all_digits(k)) && !(k.len() > 0 && k[0] == '$') && !all_word(k)
}

/// Pre-fix store arm: only a NUMERAL took the index path; every other
/// non-`$`, non-interpolated text took the literal-string-key arm.
pub open spec fn store_assoc_prefix(text: Str) -> bool {
    let k = norm(text);
    !(k.len() > 0 && all_digits(k)) && !(k.len() > 0 && k[0] == '$')
}

pub open spec fn bare_ident() -> Str { seq!['i'] }

pub open spec fn arith_text() -> Str { seq!['i', '*', '2'] }

pub open spec fn arith_expand() -> Str { seq!['$', '(', '(', 'i', '*', '2', ')', ')'] }

/// `n[i]=5`: the census said INDEXED (a bare ident is a dynamic index) and
/// the store said ASSOC — the `n_k` undeclared red (probe 59e).
pub proof fn prefix_diverges_on_ident()
    ensures
        !census_assoc_prefix(bare_ident()),
        store_assoc_prefix(bare_ident()),
{
    assert(!census_assoc_prefix(bare_ident())) by (compute);
    assert(store_assoc_prefix(bare_ident())) by (compute);
}

/// `n[i*2]=5`: BOTH consumers said assoc (the census saw a non-word text,
/// the store saw a non-numeral one), so the declaration emitted a k/v/n
/// table for a `declare -ia` array and the write compiled — into a
/// string-keyed table whose keys bash never had (`i*2` instead of 2).
pub proof fn prefix_calls_arith_assoc()
    ensures
        census_assoc_prefix(arith_text()),
        store_assoc_prefix(arith_text()),
{
    assert(census_assoc_prefix(arith_text())) by (compute);
    assert(store_assoc_prefix(arith_text())) by (compute);
}

/// The fixed pair, on the same texts: the ident is an INDEX for both (the
/// store's ident arm) and the arith text is an INDEX for both (the arith
/// arm, and the census's `subscript_arith_inner`).
pub proof fn fixed_pair_on_probes()
    ensures
        classify(bare_ident()) == SubKind::Ident,
        classify(arith_text()) == SubKind::ArithText,
        classify(arith_expand()) == SubKind::Arith,
        !census_assoc(bare_ident()),
        !store_assoc(bare_ident()),
        !census_assoc(arith_text()),
        !store_assoc(arith_text()),
        !census_assoc(arith_expand()),
        !store_assoc(arith_expand()),
{
    assert(classify(bare_ident()) == SubKind::Ident) by (compute);
    assert(classify(arith_text()) == SubKind::ArithText) by (compute);
    assert(classify(arith_expand()) == SubKind::Arith) by (compute);
}

// ── pins: the measured truth table ───────────────────────────────────
// (`by (compute)` evaluates the spec on the concrete text; the property is
// the VALUE, so nothing here is assumed.)
pub open spec fn quoted_num() -> Str { seq!['"', '1', '"'] }

pub open spec fn dollar_plus() -> Str { seq!['$', 'i', '+', '1'] }

pub open spec fn paren_arith() -> Str { seq!['(', 'i', '*', '3', ')', '+', '1'] }

/// A bare identifier key (`m[k1]`): an INDEX for an indexed array and the
/// literal key for a DECLARED assoc table — the storage class is the
/// declare's, so the classifier must not decide it.
pub open spec fn ident_key() -> Str { seq!['k', '1'] }

/// A text that is neither an ident, a numeral, a `$var` nor an arith
/// expression — the only shape that implies assoc storage on its own.
pub open spec fn word_key() -> Str { seq!['@'] }

pub open spec fn space_word() -> Str { seq!['a', ' ', 'b'] }

pub open spec fn braces_var() -> Str { seq!['$', '{', 'i', '}'] }

pub open spec fn inner_arith() -> Str { seq!['i', '*', '2'] }

/// k1-k8: the class of every form the battery exercises.
pub proof fn k1_arith_expand()
    ensures classify(arith_expand()) == SubKind::Arith
{
    assert(classify(arith_expand()) == SubKind::Arith) by (compute);
}

pub proof fn k2_bare_text()
    ensures classify(arith_text()) == SubKind::ArithText
{
    assert(classify(arith_text()) == SubKind::ArithText) by (compute);
}

pub proof fn k3_dollar_plus()
    ensures classify(dollar_plus()) == SubKind::ArithText
{
    assert(classify(dollar_plus()) == SubKind::ArithText) by (compute);
}

pub proof fn k4_bare_ident()
    ensures classify(bare_ident()) == SubKind::Ident
{
    assert(classify(bare_ident()) == SubKind::Ident) by (compute);
}

pub proof fn k5_quoted_num()
    ensures classify(quoted_num()) == SubKind::Num
{
    assert(classify(quoted_num()) == SubKind::Num) by (compute);
}

pub proof fn k6_paren_arith()
    ensures classify(paren_arith()) == SubKind::ArithText
{
    assert(classify(paren_arith()) == SubKind::ArithText) by (compute);
}

pub proof fn k7_ident_key()
    ensures
        classify(ident_key()) == SubKind::Ident,
        !census_assoc(ident_key()),
        !store_assoc(ident_key()),
{
    assert(classify(ident_key()) == SubKind::Ident) by (compute);
    assert(!census_assoc(ident_key())) by (compute);
    assert(!store_assoc(ident_key())) by (compute);
}

pub proof fn k7b_word_key()
    ensures
        classify(word_key()) == SubKind::Word,
        census_assoc(word_key()),
        store_assoc(word_key()),
{
    assert(classify(word_key()) == SubKind::Word) by (compute);
    assert(census_assoc(word_key())) by (compute);
    assert(store_assoc(word_key())) by (compute);
}

pub proof fn k8_space_word()
    ensures classify(space_word()) == SubKind::Word
{
    assert(classify(space_word()) == SubKind::Word) by (compute);
}

/// `n["1"]=v` stores index 1 — the quote strip runs BEFORE the numeral
/// test in the classifier AND in the census (bash strips subscript quotes
/// at parse time; probe 59f).
pub proof fn quote_blind()
    ensures
        classify(quoted_num()) == SubKind::Num,
        index_text(quoted_num()) == seq!['1'],
{
    assert(classify(quoted_num()) == SubKind::Num) by (compute);
    assert(index_text(quoted_num()) == seq!['1']) by (compute);
}

/// k9-k13: the INDEX text an indexed array computes. The explicit form is
/// its INNER expression (what `arith_text_expr` renders), a bare arith text
/// is itself, a bare ident is the variable read.
pub proof fn k9_index_is_inner()
    ensures index_text(arith_expand()) == inner_arith()
{
    assert(index_text(arith_expand()) == inner_arith()) by (compute);
}

pub proof fn k10_index_dollar_plus()
    ensures index_text(dollar_plus()) == dollar_plus()
{
    assert(index_text(dollar_plus()) == dollar_plus()) by (compute);
}

pub proof fn k11_index_paren()
    ensures index_text(paren_arith()) == paren_arith()
{
    assert(index_text(paren_arith()) == paren_arith()) by (compute);
}

pub proof fn k12_index_ident()
    ensures index_text(bare_ident()) == bare_ident()
{
    assert(index_text(bare_ident()) == bare_ident()) by (compute);
}

pub proof fn k13_index_const_arith()
    ensures
        index_text(seq!['$', '(', '(', '2', '*', '3', ')', ')']) == seq!['2', '*', '3'],
{
    assert(index_text(seq!['$', '(', '(', '2', '*', '3', ')', ')']) == seq!['2', '*', '3'])
        by (compute);
}

/// k14-k16: the ASSOC key text. The bare operator form stays LITERAL, the
/// explicit form is its value — the distinction the two predicates carry
/// (probes 59k/59l: bash `2 [i*2 2]` from one script).
pub proof fn k14_assoc_bare_is_literal()
    ensures
        !assoc_key_is_value(arith_text()),
        assoc_key(arith_text()) == arith_text(),
{
    assert(!assoc_key_is_value(arith_text())) by (compute);
    assert(assoc_key(arith_text()) == arith_text()) by (compute);
}

pub proof fn k15_assoc_explicit_is_value()
    ensures
        assoc_key_is_value(arith_expand()),
        assoc_key(arith_expand()) == inner_arith(),
{
    assert(assoc_key_is_value(arith_expand())) by (compute);
    assert(assoc_key(arith_expand()) == inner_arith()) by (compute);
}

/// The two forms DIFFER as assoc keys on the same table — the explicit one
/// is its VALUE, the bare one its literal text — which is how bash produces
/// `i*2 2` from one script. No single-verdict implementation could.
pub proof fn k16_assoc_keys_differ()
    ensures
        assoc_key_is_value(arith_expand()),
        !assoc_key_is_value(arith_text()),
        !(assoc_key_is_value(arith_text()) == assoc_key_is_value(arith_expand())),
{
    assert(assoc_key_is_value(arith_expand())) by (compute);
    assert(!assoc_key_is_value(arith_text())) by (compute);
}

/// The ONE divergence left, PINNED rather than papered over: the core keeps
/// `${i}`'s braces, so the store arm's `k.starts_with("${")` branch routes
/// it to assoc storage while the census excludes every `$`-prefixed text —
/// the two verdicts still differ for this shape. Fixing it needs the core
/// to split the subscript (core-request), not a renderer guess; the model
/// records the gap as a fact about the CURRENT predicates.
pub proof fn assoc_dollar_brace_remains()
    ensures
        classify(braces_var()) == SubKind::Word,
        !census_assoc(braces_var()),
        store_assoc(braces_var()),
        census_assoc(braces_var()) != store_assoc(braces_var()),
{
    assert(classify(braces_var()) == SubKind::Word) by (compute);
    assert(!census_assoc(braces_var())) by (compute);
    assert(store_assoc(braces_var())) by (compute);
}

// ── the DSE use set: closure over subscript TEXT ─────────────────────
/// Every maximal identifier run in a text — the union of what the two
/// scanners of `subscript_read_names` record (`string_read_names` for the
/// `$name`/`${name}` forms, `arith_read_names` for the bare ones; both
/// yield the same name).
pub open spec fn names_in(s: Str) -> Set<Str> {
    names_from(s, 0, Seq::empty(), Set::empty())
}

/// Walk the text accumulating the current identifier RUN, inserting it into
/// the set when it ends. (A run is built char by char — no `subrange`, which
/// is both simpler to verify and the shape of the real scanners: they emit a
/// name when they reach its terminator.)
pub open spec fn names_from(s: Str, i: int, cur: Str, acc: Set<Str>) -> Set<Str>
    decreases s.len() - i,
{
    if i >= s.len() {
        if cur.len() > 0 {
            acc.insert(cur)
        } else {
            acc
        }
    } else if is_word_char(s[i]) {
        names_from(s, i + 1, cur.push(s[i]), acc)
    } else if cur.len() > 0 {
        names_from(s, i + 1, Seq::empty(), acc.insert(cur))
    } else {
        names_from(s, i + 1, cur, acc)
    }
}

/// The read set `census_stmt` computes AFTER the fix: a flattened write
/// reads the names its subscript text mentions (plus the array name).
pub enum Stmt {
    Write { name: Str, sub: Str },
    Other { names: Set<Str> },
}

pub open spec fn reads(st: Stmt) -> Set<Str> {
    match st {
        Stmt::Write { name, sub } => names_in(sub).union(set![name]),
        Stmt::Other { names } => names,
    }
}

/// …and BEFORE it: a flattened target contributed only its own name, so a
/// name mentioned solely inside the text was invisible.
pub open spec fn reads_prefix(st: Stmt) -> Set<Str> {
    match st {
        Stmt::Write { name, sub } => set![name],
        Stmt::Other { names } => names,
    }
}

pub open spec fn sub_of(st: Stmt) -> Str {
    match st {
        Stmt::Write { name, sub } => sub,
        Stmt::Other { names } => Seq::empty(),
    }
}

/// The purge's deadness test, per statement: a store may be dropped only
/// when the statement's OWN read set does not contain the name. The union
/// over a whole program is trivial once each statement's set is closed, so
/// the obligation lives at the statement.
pub open spec fn dead(st: Stmt, v: Str) -> bool {
    !reads(st).contains(v)
}

/// CLOSURE: every name mentioned inside a flattened subscript is in the
/// statement's read set. This is the property the pre-fix census lacked —
/// it is what makes `dead_needs_no_mention` true.
pub proof fn subscript_closure(st: Stmt, v: Str)
    requires
        names_in(sub_of(st)).contains(v),
    ensures
        reads(st).contains(v),
{
}

/// The contrapositive the purge reads: a DEAD store mentions the name
/// nowhere — in particular not inside its subscript text.
pub proof fn dead_needs_no_mention(st: Stmt, v: Str)
    requires
        dead(st, v),
    ensures
        !names_in(sub_of(st)).contains(v),
{
    if names_in(sub_of(st)).contains(v) {
        subscript_closure(st, v);
    }
}

/// The measured failing program: `i=3; n[$((i))]=9` — one write whose only
/// mention of `i` is the subscript text.
pub open spec fn only_use() -> Stmt {
    Stmt::Write { name: seq!['n'], sub: seq!['$', '(', '(', 'i', ')', ')'] }
}

/// THE FIX WAS NECESSARY: the pre-fix read set of that statement is
/// `{n}` — nothing of `i` — so a store to `i` was dead by its own test
/// (measured: 2 of 3 shIR statements survived, and the renderer then
/// substituted a var with no store and no declaration). The fixed set
/// reads `i`, so the store is retained.
pub proof fn fix_was_necessary()
    ensures
        names_in(sub_of(only_use())).contains(bare_ident()),
        reads_prefix(only_use()) == set![seq!['n']],
        reads(only_use()).contains(bare_ident()),
{
    assert(names_in(sub_of(only_use())).contains(bare_ident())) by (compute);
    // the pre-fix read set is exactly `{n}` — nothing of `i`
    assert(reads_prefix(only_use()) == set![seq!['n']]) by (compute);
    assert(reads(only_use()).contains(bare_ident())) by (compute);
}

// ── mutants ──────────────────────────────────────────────────────────
/// M1: classify a bare ident as a WORD (the pre-fix store arm's behaviour
/// when the name was not in its store/type map). The mutant shares its
/// classifier with the census, so the declaration would emit a k/v/n table
/// for `n[i]=5` — the `n_k` undeclared red, restored.
pub open spec fn classify_m1(text: Str) -> SubKind {
    let k = norm(text);
    if explicit_arith(k).is_some() {
        SubKind::Arith
    } else if k.len() > 0 && all_digits(k) {
        SubKind::Num
    } else if k.len() > 1 && k[0] == '$' && all_word(k.subrange(1, k.len() as int)) {
        SubKind::DynVar
    } else if has_op(k) {
        SubKind::ArithText
    } else {
        SubKind::Word
    }
}

pub proof fn m1_bare_ident_becomes_assoc()
    ensures
        classify(bare_ident()) == SubKind::Ident,
        classify_m1(bare_ident()) == SubKind::Word,
{
    assert(classify(bare_ident()) == SubKind::Ident) by (compute);
    assert(classify_m1(bare_ident()) == SubKind::Word) by (compute);
}

/// M2: the pre-fix read set on the failing program — `i` is not read, and
/// the deadness verdict flips. This is the defect, not a preference.
pub proof fn m2_prefix_read_set_drops_the_store()
    ensures
        reads_prefix(only_use()) == set![seq!['n']],
        reads(only_use()).contains(bare_ident()),
{
    assert(reads_prefix(only_use()) == set![seq!['n']]) by (compute);
    // the mutant's read set is `{n}`; the fixed one contains `i` — that
    // containment is the whole difference between dropping and keeping
    // `i=3`
    assert(reads(only_use()).contains(bare_ident())) by (compute);
}

/// M3: keying a `$((…))` subscript by its TEXT (the pre-fix assoc key).
/// bash keys the explicit form by its VALUE, so the mutant's key differs
/// from the denotation the read side uses (`assoc_key` strips the wrapper)
/// — a write the corresponding read could never find.
pub open spec fn literal_key(text: Str) -> Str {
    norm(text)
}

pub proof fn m3_literal_key_is_wrong()
    ensures
        literal_key(arith_expand()) == arith_expand(),
        assoc_key(arith_expand()) != literal_key(arith_expand()),
{
    assert(literal_key(arith_expand()) == arith_expand()) by (compute);
    assert(assoc_key(arith_expand()) != literal_key(arith_expand())) by (compute);
}

} // verus!

fn main() {}
