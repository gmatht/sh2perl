// Standalone model of the `Output` half of `echo_fold`
// (sh2perl/src/shir_passes/echo_fold.rs: `constant_echo` → `fold_to_output`,
// with `echo_esc` as the shared `-e` table).
//
// Every backend re-implemented `echo` (`-n`/`-e`/`-E`, word joining, the
// trailing newline). The pass replaces a constant `echo` with the native
// `IrStmt::Output` node, whose contract is documented as "the value, and
// whether a newline is appended". So the law is exactly that the node prints
// what bash's echo printed:
//
//   out_bytes(fold(ws), env) == out_bytes(Echo(ws), env)   for every ws, env
//
// where the right-hand side is bash's `echo` semantics over the same words.
// The fold's table is the content, and every row is pinned by a mutant that
// goes red:
//
//   - `-n` suppresses the trailing newline (`Output.newline = false`).
//   - the trailing newline is NOT baked into the text as well: the source
//     says it plainly ("Baking `\n` into the text AND setting the flag would
//     double it"), and the doubling mutant is red.
//   - words are joined with ONE space (`echo a b` is "a b", not "ab").
//   - only LEADING literal words are flags (`echo "-n" $x` keeps its words);
//     a flag scan that keeps going past the first text word is red.
//   - `--` ends the flag scan and is itself consumed (`echo -- -n` prints
//     "-n"), so a `-n` after `--` is a WORD; treating it as a flag is red.
//   - `-e` decodes backslash escapes and `-E` turns decoding back off, last
//     one winning.
//   - the shared `echo_esc` table: `\n \t \r \\` decode, an UNKNOWN escape
//     keeps its backslash (`\q` stays `\q`), and `\c` TRUNCATES the rest of
//     the output — a table that drops the backslash or ignores `\c` is red.
//   - a word that is not a compile-time constant cannot be baked in: the
//     pass keeps the call, and folding it (an env-dependent value replaced by
//     the empty placeholder) is red under a non-empty environment.
//
// ## Hypotheses that are NOT discharged (stated, as required)
//
// - REACHABILITY / REGISTRATION: that `fold_narrow`/`fold_wide` reach the
//   statement (the container walk: If/Try/Case/loops/Function/Block/
//   Subshell/Background/Pipeline/Redirect, plus the expression positions
//   `fold_wide_expr` covers) is the walk's own business; the model states the
//   fold on a word list, not on a tree.
// - THE `-e` TABLE'S CONTRACT: `echo_esc` is "the SHARED definition of the
//   fold's `-e` semantics: the `--shir` contract and the backends' runtime
//   unescapers must agree with it". The model implements the documented rows
//   it decides (`\n \t \r \\ \c` + unknown-preserved) and takes the rest of
//   bash's table (octal/hex/`\a`/`\b`/`\f`/`\v`/`\e`) as the same contract —
//   they are more rows of one table, not a different law.
// - THE PRINTED-BYTES MODEL: the renderer prints `Output`'s value and appends
//   "\n" iff `newline`. That is the node's documented meaning.
// - THE REDIRECT HALF: `redirected_output_to_writefile` (a following
//   `WriteFile`/append mode) and `constant_printf`/`printf_s_fold` (the
//   printf table with `%s`/`%d`/widths) are separate laws over separate
//   tables; they are not modelled here.
// - THE `-e` JOIN ORDER: the pass decodes the JOINED text. bash decodes each
//   word's escapes as it prints, and the two agree because the joining
//   separator is a space that no escape can span — except that `\c` stops the
//   whole output, which is why the joined form is the one the contract
//   specifies.
// - CORPUS DOMAIN: whether a program reaches a constant echo is coverage.
//
// Verus 0.2026.10.18.7d82994, rust 1.99.1:
//   verus --triggers-mode silent --rlimit 40 echo_fold_output.rs
//   -> verification results:: NN verified, 0 errors
use vstd::prelude::*;

verus! {

// ---------- bytes ----------

pub open spec fn ch_nl() -> Seq<int> {
    seq![10]
}

pub open spec fn ch_sp() -> Seq<int> {
    seq![32]
}

pub open spec fn ch_bs() -> Seq<int> {
    seq![92]
}

// The four flag words, as byte texts.
pub open spec fn w_n() -> Seq<int> {
    seq![45, 110]
}

pub open spec fn w_e() -> Seq<int> {
    seq![45, 101]
}

pub open spec fn w_upper_e() -> Seq<int> {
    seq![45, 69]
}

pub open spec fn w_dashdash() -> Seq<int> {
    seq![45, 45]
}

// ---------- the words ----------

// A word is either a compile-time constant or a runtime value (an env read).
pub enum W {
    Lit(Seq<int>),
    Dyn(nat),
}

pub open spec fn is_lit(w: W) -> bool {
    match w {
        W::Lit(_) => true,
        W::Dyn(_) => false,
    }
}

// All words constant: the shape `fold_to_output` accepts.
pub open spec fn all_lit(ws: Seq<W>) -> bool {
    forall|i: int| 0 <= i < ws.len() ==> is_lit(ws[i])
}

// The runtime text of each word.
pub open spec fn texts(ws: Seq<W>, env: Map<nat, Seq<int>>) -> Seq<Seq<int>>
    decreases ws.len(),
{
    if ws.len() == 0 {
        Seq::empty()
    } else {
        seq![match ws[0] {
            W::Lit(s) => s,
            W::Dyn(n) => env[n],
        }] + texts(ws.subrange(1, ws.len() as int), env)
    }
}

// The compile-time text of each word; a runtime word has none (the fold is
// only built when `all_lit` holds).
pub open spec fn lit_texts(ws: Seq<W>) -> Seq<Seq<int>>
    decreases ws.len(),
{
    if ws.len() == 0 {
        Seq::empty()
    } else {
        seq![match ws[0] {
            W::Lit(s) => s,
            W::Dyn(_) => Seq::empty(),
        }] + lit_texts(ws.subrange(1, ws.len() as int))
    }
}

// ---------- the flag scan ----------

// Leading literal words only: `-n` clears the newline, `-e` turns decoding on,
// `-E` turns it off (last one wins), `--` ends the scan and is consumed.
pub open spec fn flags_acc(ws: Seq<W>, nl: bool, esc: bool) -> (int, bool, bool)
    decreases ws.len(),
{
    if ws.len() == 0 {
        (0, nl, esc)
    } else if ws[0] == W::Lit(w_n()) {
        let r = flags_acc(ws.subrange(1, ws.len() as int), false, esc);
        (r.0 + 1, r.1, r.2)
    } else if ws[0] == W::Lit(w_e()) {
        let r = flags_acc(ws.subrange(1, ws.len() as int), nl, true);
        (r.0 + 1, r.1, r.2)
    } else if ws[0] == W::Lit(w_upper_e()) {
        let r = flags_acc(ws.subrange(1, ws.len() as int), nl, false);
        (r.0 + 1, r.1, r.2)
    } else if ws[0] == W::Lit(w_dashdash()) {
        (1, nl, esc)
    } else {
        (0, nl, esc)
    }
}

pub open spec fn flags(ws: Seq<W>) -> (int, bool, bool) {
    flags_acc(ws, true, false)
}

// ---------- the shared `-e` table ----------

// Returns the decoded text AND `cut`: true when a `\c` ended the payload.
// bash's `\c` stops ALL further output, so the CALLER's trailing newline is
// part of the same decision — this is what `fold` and `echo_bytes` share.
pub open spec fn decode_cut(s: Seq<int>) -> (Seq<int>, bool)
    decreases s.len(),
{
    if s.len() == 0 {
        (Seq::empty(), false)
    } else if s[0] == 92 && s.len() >= 2 {
        if s[1] == 110 {
            // \n
            let r = decode_cut(s.subrange(2, s.len() as int));
            (ch_nl() + r.0, r.1)
        } else if s[1] == 116 {
            // \t
            let r = decode_cut(s.subrange(2, s.len() as int));
            (seq![9] + r.0, r.1)
        } else if s[1] == 114 {
            // \r
            let r = decode_cut(s.subrange(2, s.len() as int));
            (seq![13] + r.0, r.1)
        } else if s[1] == 92 {
            // \\
            let r = decode_cut(s.subrange(2, s.len() as int));
            (ch_bs() + r.0, r.1)
        } else if s[1] == 99 {
            // \c truncates the rest of the output AND the newline
            (Seq::empty(), true)
        } else {
            // an unknown escape keeps its backslash
            let r = decode_cut(s.subrange(2, s.len() as int));
            (seq![92, s[1]] + r.0, r.1)
        }
    } else {
        let r = decode_cut(s.subrange(1, s.len() as int));
        (seq![s[0]] + r.0, r.1)
    }
}

// The text alone (the escape-table pins read this).
pub open spec fn decode(s: Seq<int>) -> Seq<int> {
    decode_cut(s).0
}

// Did a `\c` stop the output?
pub open spec fn cut(s: Seq<int>) -> bool {
    decode_cut(s).1
}

// ---------- joining ----------

pub open spec fn join(ts: Seq<Seq<int>>) -> Seq<int>
    decreases ts.len(),
{
    if ts.len() == 0 {
        Seq::empty()
    } else if ts.len() == 1 {
        ts[0]
    } else {
        ts[0] + ch_sp() + join(ts.subrange(1, ts.len() as int))
    }
}

// ---------- the echo node and bash's echo ----------

pub enum S {
    Out(Seq<int>, bool),
    Echo(Seq<W>),
}

// bash's `echo`: leading flags, the remaining words joined by one space,
// backslash decoding when `-e` is on, then a newline unless `-n` OR a `\c`
// already stopped the output (`echo -e 'a\cb'` prints `a` alone).
pub open spec fn echo_bytes(ws: Seq<W>, env: Map<nat, Seq<int>>) -> Seq<int> {
    let f = flags(ws);
    let body = join(texts(ws, env).subrange(f.0, ws.len() as int));
    let d = if f.2 {
        decode_cut(body)
    } else {
        (body, false)
    };
    d.0 + (if f.1 && !d.1 {
        ch_nl()
    } else {
        Seq::empty()
    })
}

// The fold: a constant echo becomes the native Output node. Its text holds the
// joined (decoded) words and NOT the newline, which the flag carries — and a
// `\c` in the text clears that flag, exactly as it stops bash's newline.
pub open spec fn fold(ws: Seq<W>) -> S {
    if all_lit(ws) {
        let f = flags(ws);
        let body = join(lit_texts(ws).subrange(f.0, ws.len() as int));
        let d = if f.2 {
            decode_cut(body)
        } else {
            (body, false)
        };
        S::Out(d.0, f.1 && !d.1)
    } else {
        S::Echo(ws)
    }
}

// What the lowered statement prints.
pub open spec fn out_bytes(s: S, env: Map<nat, Seq<int>>) -> Seq<int> {
    match s {
        S::Out(t, nl) => t + (if nl {
            ch_nl()
        } else {
            Seq::empty()
        }),
        S::Echo(ws) => echo_bytes(ws, env),
    }
}

// ---------- lemmas ----------

// A fully-literal word list reads the same at compile time and at runtime.
proof fn lit_texts_agree(ws: Seq<W>, env: Map<nat, Seq<int>>)
    requires all_lit(ws),
    ensures texts(ws, env) == lit_texts(ws),
    decreases ws.len(),
{
    if ws.len() > 0 {
        assert(is_lit(ws[0]));
        assert forall|i: int| 0 <= i < ws.subrange(1, ws.len() as int).len()
            implies is_lit(ws.subrange(1, ws.len() as int)[i]) by {
            assert(ws.subrange(1, ws.len() as int)[i] == ws[i + 1]);
        }
        lit_texts_agree(ws.subrange(1, ws.len() as int), env);
    }
}

// The flag scan never reads past the end.
proof fn flags_acc_bounds(ws: Seq<W>, nl: bool, esc: bool)
    ensures 0 <= flags_acc(ws, nl, esc).0 <= ws.len(),
    decreases ws.len(),
{
    reveal_with_fuel(flags_acc, 2);
    if ws.len() > 0 {
        let rest = ws.subrange(1, ws.len() as int);
        assert(rest.len() == ws.len() - 1);
        if ws[0] == W::Lit(w_n()) {
            flags_acc_bounds(rest, false, esc);
        } else if ws[0] == W::Lit(w_e()) {
            flags_acc_bounds(rest, nl, true);
        } else if ws[0] == W::Lit(w_upper_e()) {
            flags_acc_bounds(rest, nl, false);
        }
    }
}

// THE LAW: the folded node prints exactly what bash's echo printed.
proof fn fold_preserves_output(ws: Seq<W>, env: Map<nat, Seq<int>>)
    ensures out_bytes(fold(ws), env) == out_bytes(S::Echo(ws), env),
{
    if all_lit(ws) {
        lit_texts_agree(ws, env);
        let f = flags(ws);
        flags_acc_bounds(ws, true, false);
        assert(texts(ws, env).subrange(f.0, ws.len() as int)
            == lit_texts(ws).subrange(f.0, ws.len() as int));
        // both sides decode the SAME payload with the SAME table, so the text
        // and the `\c` cut agree term for term
        assert(decode_cut(join(texts(ws, env).subrange(f.0, ws.len() as int)))
            == decode_cut(join(lit_texts(ws).subrange(f.0, ws.len() as int))));
    }
}

// ---------- completeness and idempotence ----------

pub open spec fn settled(s: S) -> bool {
    match s {
        S::Out(_, _) => true,
        S::Echo(ws) => !all_lit(ws),
    }
}

proof fn fold_leaves_nothing(ws: Seq<W>)
    ensures settled(fold(ws)),
{
    if all_lit(ws) {
        assert(fold(ws) is Out);
    }
}

pub open spec fn fold_s(s: S) -> S {
    match s {
        S::Out(_, _) => s,
        S::Echo(ws) => fold(ws),
    }
}

proof fn fold_is_idempotent(s: S)
    ensures fold_s(fold_s(s)) == fold_s(s),
{
    match s {
        S::Out(_, _) => {}
        S::Echo(ws) => {}
    }
}


// ---------- the naive alternatives go red ----------

// (1) bake the newline into the text AND keep the flag: every capture doubles.
pub open spec fn fold_bake_newline(ws: Seq<W>) -> S {
    if all_lit(ws) {
        let f = flags(ws);
        let body = join(lit_texts(ws).subrange(f.0, ws.len() as int));
        let t = if f.2 {
            decode(body)
        } else {
            body
        };
        S::Out(t + ch_nl(), f.1)
    } else {
        S::Echo(ws)
    }
}

// (2) join the words with no separator.
pub open spec fn join_no_sep(ts: Seq<Seq<int>>) -> Seq<int>
    decreases ts.len(),
{
    if ts.len() == 0 {
        Seq::empty()
    } else {
        ts[0] + join_no_sep(ts.subrange(1, ts.len() as int))
    }
}

// (3) ignore `-e` (no backslash decoding).
pub open spec fn fold_no_esc(ws: Seq<W>) -> S {
    if all_lit(ws) {
        let f = flags(ws);
        S::Out(join(lit_texts(ws).subrange(f.0, ws.len() as int)), f.1)
    } else {
        S::Echo(ws)
    }
}

// (4) let the flag scan run PAST `--`.
pub open spec fn flags_past_dashdash(ws: Seq<W>) -> (int, bool, bool)
    decreases ws.len(),
{
    if ws.len() == 0 {
        (0, true, false)
    } else if ws[0] == W::Lit(w_dashdash()) {
        // MUTANT: `--` is consumed but the scan keeps going, so a following
        // `-n` is still read as a flag
        let r = flags_past_dashdash(ws.subrange(1, ws.len() as int));
        (r.0 + 1, r.1, r.2)
    } else if ws[0] == W::Lit(w_n()) {
        let r = flags_past_dashdash(ws.subrange(1, ws.len() as int));
        (r.0 + 1, false, r.2)
    } else if ws[0] == W::Lit(w_e()) {
        let r = flags_past_dashdash(ws.subrange(1, ws.len() as int));
        (r.0 + 1, r.1, true)
    } else if ws[0] == W::Lit(w_upper_e()) {
        let r = flags_past_dashdash(ws.subrange(1, ws.len() as int));
        (r.0 + 1, r.1, false)
    } else {
        (0, true, false)
    }
}

pub open spec fn fold_past_dashdash(ws: Seq<W>) -> S {
    if all_lit(ws) {
        let f = flags_past_dashdash(ws);
        let body = join(lit_texts(ws).subrange(f.0, ws.len() as int));
        let t = if f.2 {
            decode(body)
        } else {
            body
        };
        S::Out(t, f.1)
    } else {
        S::Echo(ws)
    }
}

// (5) a `\c` that does not truncate (an unknown escape instead).
pub open spec fn decode_keep_c(s: Seq<int>) -> Seq<int>
    decreases s.len(),
{
    if s.len() == 0 {
        Seq::empty()
    } else if s[0] == 92 && s.len() >= 2 {
        if s[1] == 110 {
            ch_nl() + decode_keep_c(s.subrange(2, s.len() as int))
        } else if s[1] == 116 {
            seq![9] + decode_keep_c(s.subrange(2, s.len() as int))
        } else if s[1] == 114 {
            seq![13] + decode_keep_c(s.subrange(2, s.len() as int))
        } else if s[1] == 92 {
            ch_bs() + decode_keep_c(s.subrange(2, s.len() as int))
        } else {
            seq![92, s[1]] + decode_keep_c(s.subrange(2, s.len() as int))
        }
    } else {
        seq![s[0]] + decode_keep_c(s.subrange(1, s.len() as int))
    }
}

// (6) an unknown escape loses its backslash.
pub open spec fn decode_drop_bs(s: Seq<int>) -> Seq<int>
    decreases s.len(),
{
    if s.len() == 0 {
        Seq::empty()
    } else if s[0] == 92 && s.len() >= 2 {
        if s[1] == 110 {
            ch_nl() + decode_drop_bs(s.subrange(2, s.len() as int))
        } else if s[1] == 99 {
            Seq::empty()
        } else {
            seq![s[1]] + decode_drop_bs(s.subrange(2, s.len() as int))
        }
    } else {
        seq![s[0]] + decode_drop_bs(s.subrange(1, s.len() as int))
    }
}

// (7) fold a runtime word too, using the compile-time placeholder.
pub open spec fn fold_dyn_as_lit(ws: Seq<W>) -> S {
    let f = flags(ws);
    let body = join(lit_texts(ws).subrange(f.0, ws.len() as int));
    let t = if f.2 {
        decode(body)
    } else {
        body
    };
    S::Out(t, f.1)
}

// (8) an identity pass: the echo call survives.
pub open spec fn fold_identity(ws: Seq<W>) -> S {
    S::Echo(ws)
}

// (9) keep the newline even when `\c` cut the payload — the pre-repair shape
// (`fold_to_output` set `newline: nl` regardless of the unescaper's verdict).
pub open spec fn fold_keep_newline_after_cut(ws: Seq<W>) -> S {
    if all_lit(ws) {
        let f = flags(ws);
        let body = join(lit_texts(ws).subrange(f.0, ws.len() as int));
        let d = if f.2 {
            decode_cut(body)
        } else {
            (body, false)
        };
        S::Out(d.0, f.1)
    } else {
        S::Echo(ws)
    }
}

proof fn newline_flag_is_load_bearing()
    ensures ({
        let ws = seq![W::Lit(w_n()), W::Lit(seq![104, 105])];
        let env = Map::<nat, Seq<int>>::empty();
        out_bytes(S::Echo(ws), env) == seq![104, 105]
            && out_bytes(fold(ws), env) == seq![104, 105]
            && out_bytes(fold_bake_newline(ws), env) == seq![104, 105, 10]
            && out_bytes(fold_bake_newline(ws), env) != out_bytes(S::Echo(ws), env)
    }),
{
    reveal_with_fuel(flags_acc, 4);
    reveal_with_fuel(lit_texts, 4);
    reveal_with_fuel(texts, 4);
    reveal_with_fuel(join, 4);
    reveal_with_fuel(decode_cut, 4);
}

proof fn space_join_is_load_bearing()
    ensures ({
        let ws = seq![W::Lit(seq![97]), W::Lit(seq![98])];
        let env = Map::<nat, Seq<int>>::empty();
        out_bytes(S::Echo(ws), env) == seq![97, 32, 98, 10]
            && out_bytes(fold(ws), env) == seq![97, 32, 98, 10]
            && join_no_sep(lit_texts(ws)) == seq![97, 98]
            && join_no_sep(lit_texts(ws)) != join(lit_texts(ws))
    }),
{
    reveal_with_fuel(flags_acc, 4);
    reveal_with_fuel(lit_texts, 4);
    reveal_with_fuel(texts, 4);
    reveal_with_fuel(join, 4);
    reveal_with_fuel(join_no_sep, 4);
    reveal_with_fuel(decode_cut, 4);
}

proof fn dash_e_is_load_bearing()
    ensures ({
        // echo -e 'a\nb' prints a, a real newline, b — and the trailing newline
        let ws = seq![W::Lit(w_e()), W::Lit(seq![97, 92, 110, 98])];
        let env = Map::<nat, Seq<int>>::empty();
        out_bytes(S::Echo(ws), env) == seq![97, 10, 98, 10]
            && out_bytes(fold(ws), env) == seq![97, 10, 98, 10]
            // ignoring -e leaves the backslash-n spelling
            && out_bytes(fold_no_esc(ws), env) == seq![97, 92, 110, 98, 10]
            && out_bytes(fold_no_esc(ws), env) != out_bytes(S::Echo(ws), env)
    }),
{
    reveal_with_fuel(flags_acc, 4);
    reveal_with_fuel(lit_texts, 4);
    reveal_with_fuel(texts, 4);
    reveal_with_fuel(join, 4);
    reveal_with_fuel(decode_cut, 4);
}

proof fn dashdash_is_load_bearing()
    ensures ({
        // echo -- -n prints "-n" and a newline: the dashdash consumes itself
        // and the next word is a WORD, not a flag
        let ws = seq![W::Lit(w_dashdash()), W::Lit(w_n())];
        let env = Map::<nat, Seq<int>>::empty();
        out_bytes(S::Echo(ws), env) == seq![45, 110, 10]
            && out_bytes(fold(ws), env) == seq![45, 110, 10]
            // scanning past `--` eats the word and clears the newline
            && out_bytes(fold_past_dashdash(ws), env) == Seq::<int>::empty()
            && out_bytes(fold_past_dashdash(ws), env) != out_bytes(S::Echo(ws), env)
    }),
{
    reveal_with_fuel(flags_acc, 4);
    reveal_with_fuel(flags_past_dashdash, 4);
    reveal_with_fuel(lit_texts, 4);
    reveal_with_fuel(texts, 4);
    reveal_with_fuel(join, 4);
    reveal_with_fuel(decode_cut, 4);
    assert(all_lit(seq![W::Lit(w_dashdash()), W::Lit(w_n())]));
    assert(flags(seq![W::Lit(w_dashdash()), W::Lit(w_n())]) == (1, true, false));
    assert(lit_texts(seq![W::Lit(w_dashdash()), W::Lit(w_n())]) == seq![w_dashdash(), w_n()]);
    assert(join(seq![w_n()]) == w_n());
    assert(flags_past_dashdash(seq![W::Lit(w_dashdash()), W::Lit(w_n())]) == (2, false, false));
}

// `\c` ends the payload AND the newline — bash's measured bytes (`od -c` of
// its stdout):
//   echo -e 'a\cb'    -> 97                     (`a`, nothing else)
//   echo -e 'ab\ccd'  -> 97 98                  (`ab`)
//   echo -e 'ab\c'    -> 97 98                  (`ab`, no newline)
//   echo -e 'a\cb\c'  -> 97                     (`a`)
//   echo -e '\c'      -> (nothing at all)
proof fn escape_c_truncation_is_load_bearing()
    ensures ({
        let ws = seq![W::Lit(w_e()), W::Lit(seq![97, 92, 99, 98])];
        let env = Map::<nat, Seq<int>>::empty();
        out_bytes(S::Echo(ws), env) == seq![97]
            && out_bytes(fold(ws), env) == seq![97]
            && decode_keep_c(seq![97, 92, 99, 98]) == seq![97, 92, 99, 98]
            && decode_keep_c(seq![97, 92, 99, 98]) != decode(seq![97, 92, 99, 98])
    }),
{
    reveal_with_fuel(flags_acc, 4);
    reveal_with_fuel(lit_texts, 4);
    reveal_with_fuel(texts, 4);
    reveal_with_fuel(join, 4);
    reveal_with_fuel(decode_cut, 4);
    reveal_with_fuel(decode_keep_c, 4);
    assert(all_lit(seq![W::Lit(w_e()), W::Lit(seq![97, 92, 99, 98])]));
    assert(flags(seq![W::Lit(w_e()), W::Lit(seq![97, 92, 99, 98])]) == (1, true, true));
    assert(decode(seq![97, 92, 99, 98]) == seq![97]);
    assert(cut(seq![97, 92, 99, 98]));
    assert(decode_keep_c(seq![97, 92, 99, 98]) == seq![97, 92, 99, 98]);
}

// The four neighbouring shapes, each folding to exactly bash's bytes — the
// cut position must not need a special case, so the same law covers them.
proof fn dash_c_shapes_match_bash()
    ensures ({
        let env = Map::<nat, Seq<int>>::empty();
        let e = W::Lit(w_e());
        // 1. mid-text: `echo -e 'ab\ccd'` -> `ab`
        out_bytes(fold(seq![e, W::Lit(seq![97, 98, 92, 99, 99, 100])]), env) == seq![97, 98]
            // 2. the last escape: `echo -e 'ab\c'` -> `ab`
            && out_bytes(fold(seq![e, W::Lit(seq![97, 98, 92, 99])]), env) == seq![97, 98]
            // 3. after an already-truncated payload: `echo -e 'a\cb\c'` -> `a`
            && out_bytes(fold(seq![e, W::Lit(seq![97, 92, 99, 98, 92, 99])]), env) == seq![97]
            // 4. `\c` is the only escape: `echo -e '\c'` -> nothing at all
            && out_bytes(fold(seq![e, W::Lit(seq![92, 99])]), env) == Seq::<int>::empty()
            // control: the same payloads WITHOUT `\c` keep bash's newline
            && out_bytes(fold(seq![e, W::Lit(seq![97, 98])]), env) == seq![97, 98, 10]
            && out_bytes(fold(seq![W::Lit(seq![97, 98])]), env) == seq![97, 98, 10]
    }),
{
    reveal_with_fuel(flags_acc, 6);
    reveal_with_fuel(lit_texts, 6);
    reveal_with_fuel(texts, 6);
    reveal_with_fuel(join, 6);
    reveal_with_fuel(decode_cut, 8);
    assert(all_lit(seq![W::Lit(w_e()), W::Lit(seq![97, 98, 92, 99, 99, 100])]));
    assert(all_lit(seq![W::Lit(w_e()), W::Lit(seq![97, 98, 92, 99])]));
    assert(all_lit(seq![W::Lit(w_e()), W::Lit(seq![97, 92, 99, 98, 92, 99])]));
    assert(all_lit(seq![W::Lit(w_e()), W::Lit(seq![92, 99])]));
    assert(decode_cut(seq![97, 98, 92, 99, 99, 100]) == (seq![97, 98], true));
    assert(decode_cut(seq![97, 98, 92, 99]) == (seq![97, 98], true));
    assert(decode_cut(seq![97, 92, 99, 98, 92, 99]) == (seq![97], true));
    assert(decode_cut(seq![92, 99]) == (Seq::<int>::empty(), true));
    assert(decode_cut(seq![97, 98]) == (seq![97, 98], false));
}

proof fn unknown_escape_is_load_bearing()
    ensures ({
        let ws = seq![W::Lit(w_e()), W::Lit(seq![97, 92, 113, 98])];
        let env = Map::<nat, Seq<int>>::empty();
        // an unknown escape keeps its backslash
        out_bytes(S::Echo(ws), env) == seq![97, 92, 113, 98, 10]
            && out_bytes(fold(ws), env) == seq![97, 92, 113, 98, 10]
            && decode_drop_bs(seq![97, 92, 113, 98]) == seq![97, 113, 98]
            && decode_drop_bs(seq![97, 92, 113, 98]) != decode(seq![97, 92, 113, 98])
    }),
{
    reveal_with_fuel(flags_acc, 4);
    reveal_with_fuel(lit_texts, 4);
    reveal_with_fuel(texts, 4);
    reveal_with_fuel(join, 4);
    reveal_with_fuel(decode_cut, 4);
    reveal_with_fuel(decode_drop_bs, 4);
}

proof fn runtime_word_is_load_bearing()
    ensures ({
        let ws = seq![W::Dyn(0)];
        let env = Map::<nat, Seq<int>>::empty().insert(0, seq![120]);
        out_bytes(S::Echo(ws), env) == seq![120, 10]
            && out_bytes(fold(ws), env) == seq![120, 10]
            // the fold-a-runtime-word mutant prints the empty placeholder
            && out_bytes(fold_dyn_as_lit(ws), env) == seq![10]
            && out_bytes(fold_dyn_as_lit(ws), env) != out_bytes(S::Echo(ws), env)
    }),
{
    reveal_with_fuel(flags_acc, 4);
    reveal_with_fuel(lit_texts, 4);
    reveal_with_fuel(texts, 4);
    reveal_with_fuel(join, 4);
    reveal_with_fuel(decode_cut, 4);
}

// The newline after a cut is load-bearing: the mutant that keeps it writes
// `a\n` where bash writes `a`, so `fold_preserves_output` refuses it.
proof fn cut_newline_is_load_bearing()
    ensures ({
        let ws = seq![W::Lit(w_e()), W::Lit(seq![97, 92, 99, 98])];
        let env = Map::<nat, Seq<int>>::empty();
        out_bytes(fold(ws), env) == seq![97]
            && out_bytes(fold_keep_newline_after_cut(ws), env) == seq![97, 10]
            && out_bytes(fold_keep_newline_after_cut(ws), env) != out_bytes(S::Echo(ws), env)
    }),
{
    reveal_with_fuel(flags_acc, 4);
    reveal_with_fuel(lit_texts, 4);
    reveal_with_fuel(texts, 4);
    reveal_with_fuel(join, 4);
    reveal_with_fuel(decode_cut, 4);
    assert(decode_cut(seq![97, 92, 99, 98]) == (seq![97], true));
    assert(flags(seq![W::Lit(w_e()), W::Lit(seq![97, 92, 99, 98])]) == (1, true, true));
}

proof fn identity_is_not_a_fold()
    ensures !settled(fold_identity(seq![W::Lit(seq![104, 105])]))
        && settled(fold(seq![W::Lit(seq![104, 105])])),
{
}

// ---------- pins: the source's tests ----------

// literal_echo_folds_to_output (and quoted_literal_interpolate_folds: an
// all-literal word list is the same shape).
proof fn pin_literal_echo_folds()
    ensures fold(seq![W::Lit(seq![104, 105])]) == S::Out(seq![104, 105], true),
{
    reveal_with_fuel(flags_acc, 4);
    reveal_with_fuel(lit_texts, 4);
    reveal_with_fuel(join, 4);
}

// flags_and_join: `-n` and the one-space join.
proof fn pin_flags_and_join()
    ensures fold(seq![W::Lit(w_n()), W::Lit(seq![97]), W::Lit(seq![98])])
        == S::Out(seq![97, 32, 98], false),
{
    reveal_with_fuel(flags_acc, 4);
    reveal_with_fuel(lit_texts, 4);
    reveal_with_fuel(join, 4);
    assert(all_lit(seq![W::Lit(w_n()), W::Lit(seq![97]), W::Lit(seq![98])]));
    assert(flags(seq![W::Lit(w_n()), W::Lit(seq![97]), W::Lit(seq![98])]) == (1, false, false));
    assert(lit_texts(seq![W::Lit(w_n()), W::Lit(seq![97]), W::Lit(seq![98])])
        == seq![w_n(), seq![97], seq![98]]);
    assert(join(seq![seq![97], seq![98]]) == seq![97, 32, 98]);
}

// dash_e_unescapes.
proof fn pin_dash_e_unescapes()
    ensures fold(seq![W::Lit(w_e()), W::Lit(seq![97, 92, 110, 98])])
        == S::Out(seq![97, 10, 98], true),
{
    reveal_with_fuel(flags_acc, 4);
    reveal_with_fuel(lit_texts, 4);
    reveal_with_fuel(join, 4);
    reveal_with_fuel(decode_cut, 4);
    assert(all_lit(seq![W::Lit(w_e()), W::Lit(seq![97, 92, 110, 98])]));
    assert(flags(seq![W::Lit(w_e()), W::Lit(seq![97, 92, 110, 98])]) == (1, true, true));
    assert(decode(seq![97, 92, 110, 98]) == seq![97, 10, 98]);
}

// expansion_word_does_not_fold and dynamic_output_keeps_the_expression: a
// runtime word keeps the call.
proof fn pin_dynamic_keeps_the_call()
    ensures fold(seq![W::Dyn(0)]) == S::Echo(seq![W::Dyn(0)])
        && fold(seq![W::Lit(seq![97]), W::Dyn(0)]) == S::Echo(seq![W::Lit(seq![97]), W::Dyn(0)]),
{
}

// output_without_newline_stays_without_one.
proof fn pin_output_without_newline()
    ensures ({
        let env = Map::<nat, Seq<int>>::empty();
        out_bytes(fold(seq![W::Lit(w_n()), W::Lit(seq![104, 105])]), env)
            == seq![104, 105]
            && out_bytes(fold(seq![W::Lit(seq![104, 105])]), env) == seq![104, 105, 10]
    }),
{
    reveal_with_fuel(flags_acc, 4);
    reveal_with_fuel(lit_texts, 4);
    reveal_with_fuel(join, 4);
    reveal_with_fuel(decode_cut, 4);
}

// echo_esc_matches_bash_common_cases (the four rows the model decides).
proof fn pin_echo_esc_rows()
    ensures decode(seq![97, 92, 110, 98]) == seq![97, 10, 98]
        && decode(seq![97, 92, 116, 98]) == seq![97, 9, 98]
        && decode(seq![97, 92, 92, 98]) == seq![97, 92, 98]
        && decode(seq![97, 92, 99, 98]) == seq![97]
        // only `\c` cuts; the other rows leave the newline alone
        && cut(seq![97, 92, 99, 98])
        && !cut(seq![97, 92, 110, 98])
        && !cut(seq![97, 92, 116, 98])
        && !cut(seq![97, 92, 92, 98])
        && !cut(Seq::<int>::empty()),
{
    reveal_with_fuel(decode_cut, 6);
}

fn main() {}
}
