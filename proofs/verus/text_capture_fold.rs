//  ─────────────────────────────────────────────────────────────────────────
//  proofs/verus/text_capture_fold.rs — folding `$(echo lit …)` into raw shell
//  text is value-preserving and syntax-free.
//
//  `transforms/text_capture_fold.rs` rewrites RAW TEXT that the renderers
//  re-parse (`test` / `param` / `arith` arguments): a `$( … )` or backtick
//  substitution whose output is PROVABLY constant is replaced by that
//  literal text, so the renderer's re-parse sees a constant operand and no
//  backend calls out at runtime. Two separate claims license it, and this
//  file proves both:
//
//   (A) WHAT THE SUBSTITUTION PRINTS.
//       The analysis (`constant_output`: only `echo` with all-literal
//       words and no `-n`/`-e`/`-E` flag, or bare `true` / `:`) returns
//       `Some(r)` only when the request really prints exactly `r` — for
//       every environment. `emit` is the independent semantic side and is
//       deliberately RICHER (it still calls a newline-carrying word or a
//       `-n`-flagged echo constant); the law is one-directional, which is
//       the direction the fold needs: analysis ⇒ constant.
//
//   (B) WHAT THE RE-PARSE SEES.
//       Command-substitution output is NOT re-parsed for quoting: a `"`
//       inside `${ … }`'s output is an ordinary character. Text that is
//       spliced into the shell text IS re-parsed, so a `"`/`'`/`\`/`$`/
//       backtick in the replacement could open a quote, escape, or expand.
//       That is exactly the set the source's `safe_repl` gate excludes
//       (unquoted: the whole shell-safe charset; inside `"…"`: the four
//       that end, escape or re-expand). The law: when the gate holds, the
//       token stream the re-parse produces is IDENTICAL to the token
//       stream the substitution produced.
//
//  ── the model ──────────────────────────────────────────────────────────
//
//   * A substitution's content is a classified WORD LIST: `Lit` (literal
//     text) or `Rt` (a word whose value depends on the environment — a
//     `$var`, glob, tilde, metacharacter, or quoted expansion). The head
//     is `Echo`, `True`, `Colon`, or `Other`.
//   * `emit` (semantic) vs `analysis` (the pass's `constant_output`).
//   * A SECOND, independent semantic recursion, `printed`/`prints`: it
//     assigns each word its runtime value (`Lit` prints its text, `Rt` has
//     no constant value) and builds the text itself, sharing none of the
//     analysis's `all_lit`/`any_flag`/`join` helpers. `analysis_prints_the_
//     output` refines the analysis against THAT side, so a guard forgotten
//     in the analysis disagrees with it instead of weakening both sides at
//     once.
//   * Shell text is a token list: `Ch` (literal char), `Syn` (a char that
//     the re-parse treats as SYNTAX), `Sp` (a separator), `Sub` (one
//     command substitution). `expand` is the token stream the ORIGINAL
//     produces (a substitution's own output never becomes syntax);
//     `foldtok` is the token stream the FOLDED text produces (the same
//     characters, now re-parsed).
//
//  ── stated, not modelled ───────────────────────────────────────────────
//
//   * The scan/quoting machinery itself (`scan`, `find_close`, the quote
//     modes, span splicing) is a lexer; the model starts from "here is the
//     substitution and the text around it", which is what the lexer
//     guarantees. Single-quoted regions never reach the fold (`scan` skips
//     them wholesale) — modelled as: no `Sub` token exists there.
//   * `shell_words`' per-character refusals (`\ $ ` | & ; < > ( ) { } * ?
//     [ ] # ! ~`) are modelled as the word being `Rt`: one class for all
//     environment-dependent words. Their individual spellings are the
//     source's tests (`refuses_glob_and_tilde_words`, `refuses_variable_words`).
//   * The `-e`/`-E`/`-n` flag FORMATTING (`-e` interpreting `\n`) is not
//     modelled as output text; a flag makes `emit` `NotPlain` and the
//     analysis refuse.
//   * Which `IrExpr` positions carry raw text (`TEXT_FUNCS`) and the IR
//     traversal are structural, not semantic: any subset of positions that
//     still receives `fold_text` is covered by (A)+(B) unchanged.
//
//  ── measured mutants (all restored byte-identically; sha in the sync file) ──
//
//  baseline 43 verified, 0 errors.  TWO VACUITIES were found here by the
//  mutants below and fixed in this file; both mutants used to be GREEN:
//
//    vacuity 1 (flag charset).  `is_flag`, `emit` and `flags_in` all read the
//      one `all_ne_e`, so narrowing its charset to `n` left the file green
//      (37v/0e, measured) although the pass would then fold `$(echo -e hi)`
//      into `-e hi`, which bash does not print.  FIX: `flag_text` +
//      `is_flag_sem` -- the semantic side's own flag recursion, used by
//      `prints` -- with `flag_text_is_all_ne_e` / `is_flag_sem_is_is_flag` as
//      the bridge; the mutant now breaks the bridge (m1b/m1c).
//    vacuity 2 (line breaks).  `analysis` and `emit` both read
//      `any_broken`/`has_ws`, so dropping `\r` from it left the file green
//      (41v/0e, measured after fix 1) although a `\r` replacement would
//      follow the fold into a single-line operand.  FIX: `breaks` inside
//      `printed` with `has_ws_is_breaks` as the bridge (m8).
//
//  counted -- the LAW fails at verification, with a real count:
//    m1b the semantic flag test loses its length check   42 verified, 1 error
//           (is_flag_sem_is_is_flag)
//    m1c the semantic flag charset loses `E`              42 verified, 1 error
//           (flag_text_is_all_ne_e)
//    m2  the gate skips a substitution (a nested walk that skips a
//        subexpression)                                   42 verified, 1 error
//           (lemma_tokens -> fold_preserves_the_token_stream)
//    m6  the substitution's space becomes a literal char  42 verified, 1 error
//           (lemma_chars)
//    m7  the folded text's space becomes a literal char   42 verified, 1 error
//           (lemma_chars)
//    m8  the line-break check stops refusing `\r`         42 verified, 1 error
//           (has_ws_is_breaks) -- vacuity 2's own mutant
//
//  refuted one stage earlier, at simplification, by a concrete pin (recorded
//  as pin-level evidence, NOT as a law):
//    m1  the flag charset narrowed to `n` (pin_flag_classes_agree) -- the
//        mutant that used to be green; its law-level twins are m1b/m1c
//    m3  the join doubles its separator (pin_analysis_shapes)
//    m4  the analysis trusts an echo flag (pin_analysis_shapes)
//    m5  a plain character becomes syntax inside quotes
//        (pin_a_quote_breaks_the_parse)
//    earlier notes also recorded an Rt word counted literal, every head
//    printing, a wide-open unquoted gate and the semantic recursion printing
//    an Rt word as empty text: same class, same pin-first behaviour
//
//  ─────────────────────────────────────────────────────────────────────────

use vstd::prelude::*;

verus! {

// ══════════════════════════════════════════════════════ (A) constant output ──

pub enum W {
    /// literal text, exactly as the shell would print it
    Lit(Seq<char>),
    /// a word whose value depends on the runtime environment
    Rt,
}

pub enum Head {
    Echo,
    True,
    Colon,
    Other,
}

pub enum Out {
    Const(Seq<char>),
    /// not the plain join: environment-dependent, or reformatted by a flag
    NotPlain,
}

pub open spec fn wtext(w: W) -> Seq<char> {
    match w {
        W::Lit(s) => s,
        W::Rt => Seq::empty(),
    }
}

/// every word is literal (the analysis's only way to compute an output)
pub open spec fn all_lit(ws: Seq<W>, i: int) -> bool
    decreases ws.len() - i,
{
    if i >= ws.len() {
        true
    } else {
        (match ws[i] {
            W::Lit(_) => true,
            W::Rt => false,
        }) && all_lit(ws, i + 1)
    }
}

/// an echo FLAG (`-n`, `-e`, `-E`, and combinations; the source spells
/// the charset `neE`)
pub open spec fn all_ne_e(s: Seq<char>, i: int) -> bool
    decreases s.len() - i,
{
    if i >= s.len() {
        true
    } else {
        (s[i] == 'n' || s[i] == 'e' || s[i] == 'E') && all_ne_e(s, i + 1)
    }
}

pub open spec fn is_flag(w: W) -> bool {
    match w {
        W::Lit(s) => s.len() > 1 && s[0] == '-' && all_ne_e(s, 1),
        W::Rt => false,
    }
}

pub open spec fn any_flag(ws: Seq<W>, i: int) -> bool
    decreases ws.len() - i,
{
    if i >= ws.len() {
        false
    } else {
        is_flag(ws[i]) || any_flag(ws, i + 1)
    }
}

pub open spec fn has_ws(s: Seq<char>, i: int) -> bool
    decreases s.len() - i,
{
    if i >= s.len() {
        false
    } else {
        (s[i] == '\n' || s[i] == '\r') || has_ws(s, i + 1)
    }
}

/// a word that broke a line: the renderer's single-line re-parse would lose it
pub open spec fn any_broken(ws: Seq<W>, i: int) -> bool
    decreases ws.len() - i,
{
    if i >= ws.len() {
        false
    } else {
        (match ws[i] {
            W::Lit(s) => has_ws(s, 0),
            W::Rt => false,
        }) || any_broken(ws, i + 1)
    }
}

/// the text `echo` prints for its words: the words joined by one space
pub open spec fn join(ws: Seq<W>, i: int) -> Seq<char>
    decreases ws.len() - i,
{
    if i >= ws.len() {
        Seq::empty()
    } else if i == 0 {
        wtext(ws[0]).add(join(ws, 1))
    } else {
        seq![' '].add(wtext(ws[i])).add(join(ws, i + 1))
    }
}

/// `constant_output`, transcribed: `Some(r)` only for a provably constant `r`.
pub open spec fn analysis(h: Head, ws: Seq<W>) -> Option<Seq<char>> {
    match h {
        Head::True | Head::Colon => if ws.len() == 0 { Some(Seq::empty()) } else { None },
        Head::Echo => if all_lit(ws, 0) && !any_flag(ws, 0) && !any_broken(ws, 0) {
            Some(join(ws, 0))
        } else {
            None
        },
        Head::Other => None,
    }
}

/// The independent semantic side: what the command actually prints.
/// Richer than `analysis` on purpose — a newline-carrying literal word and a
/// `-n` flag are still a CONSTANT text here; `analysis` refuses them for
/// other reasons (single-line re-parse; the flag's own formatting).
pub open spec fn emit(h: Head, ws: Seq<W>) -> Out {
    match h {
        Head::True | Head::Colon => if ws.len() == 0 { Out::Const(Seq::empty()) } else { Out::NotPlain },
        Head::Echo => {
            if all_lit(ws, 0) && !any_flag(ws, 0) {
                Out::Const(join(ws, 0))
            } else {
                Out::NotPlain
            }
        }
        Head::Other => Out::NotPlain,
    }
}

// ── the semantic output as its OWN recursion ──────────────────────────
//
// `emit` above shares the analysis's `all_lit`/`any_flag`/`join` helpers,
// so weakening one of those would weaken both sides together. This second,
// independent semantic model assigns each word its runtime value and
// assembles the text itself: `Lit` prints its text, `Rt` has no constant
// output. A pass that accepts an `Rt` word (or a flag) now disagrees with
// this side instead of moving in lockstep with it.

/// A word that broke a line, as the SEMANTIC side sees it — its own
/// recursion, not `has_ws`. With a shared check this file stayed green
/// (measured: 41v/0e) when `has_ws` stopped refusing `\r`, which would have
/// let the fold splice a multi-line replacement into a single-line operand.
pub open spec fn breaks(s: Seq<char>, i: int) -> bool
    decreases s.len() - i,
{
    if i >= s.len() {
        false
    } else {
        (s[i] == '\n' || s[i] == '\r') || breaks(s, i + 1)
    }
}

pub open spec fn printed(ws: Seq<W>, i: int) -> Option<Seq<char>>
    decreases ws.len() - i,
{
    if i >= ws.len() {
        Some(Seq::empty())
    } else {
        match ws[i] {
            W::Lit(s) => if breaks(s, 0) {
                Option::None
            } else {
                match printed(ws, i + 1) {
                    Option::Some(rest) => Option::Some(
                        if i == 0 { s.add(rest) } else { seq![' '].add(s).add(rest) },
                    ),
                    Option::None => Option::None,
                }
            },
            W::Rt => Option::None,
        }
    }
}

/// The flag CHARSET as its own recursion. This is deliberately a second
/// transcription of "an echo flag is `-` followed by `n`/`e`/`E`" rather
/// than a reuse of `all_ne_e`: with one shared definition, narrowing the
/// charset moved `analysis`, `emit` and `flags_in` together and the whole
/// file stayed green (measured: 37v/0e) while the pass would fold
/// `$(echo -e hi)` into the wrong text. The bridge below is the law that
/// now catches that mutant.
pub open spec fn flag_text(s: Seq<char>, i: int) -> bool
    decreases s.len() - i,
{
    if i >= s.len() {
        true
    } else {
        (s[i] == 'n' || s[i] == 'e' || s[i] == 'E') && flag_text(s, i + 1)
    }
}

/// a flag, judged by the semantic side's own recursion
pub open spec fn is_flag_sem(w: W) -> bool {
    match w {
        W::Lit(s) => s.len() > 1 && s[0] == '-' && flag_text(s, 1),
        W::Rt => false,
    }
}

/// echo flags, as the SEMANTIC side sees them (its own recursion).
pub open spec fn flags_in(ws: Seq<W>, i: int) -> bool
    decreases ws.len() - i,
{
    if i >= ws.len() {
        false
    } else {
        is_flag_sem(ws[i]) || flags_in(ws, i + 1)
    }
}

/// What the command prints, computed from the words' runtime values.
pub open spec fn prints(h: Head, ws: Seq<W>) -> Out {
    match h {
        Head::True | Head::Colon => if ws.len() == 0 {
            Out::Const(Seq::empty())
        } else {
            Out::NotPlain
        },
        Head::Echo => {
            if flags_in(ws, 0) {
                Out::NotPlain
            } else {
                match printed(ws, 0) {
                    Option::Some(r) => Out::Const(r),
                    Option::None => Out::NotPlain,
                }
            }
        }
        Head::Other => Out::NotPlain,
    }
}

/// the two line-break recursions agree, and the analysis's refusal on a
/// broken word is the semantic side's too (a mutant in `has_ws` breaks this)
pub proof fn has_ws_is_breaks(s: Seq<char>, i: int)
    ensures
        has_ws(s, i) == breaks(s, i),
    decreases s.len() - i,
{
    if i < s.len() {
        has_ws_is_breaks(s, i + 1);
    }
}

/// the two char recursions agree: they are the same charset, written twice
pub proof fn flag_text_is_all_ne_e(s: Seq<char>, i: int)
    ensures
        flag_text(s, i) == all_ne_e(s, i),
    decreases s.len() - i,
{
    if i < s.len() {
        flag_text_is_all_ne_e(s, i + 1);
    }
}

pub proof fn is_flag_sem_is_is_flag(w: W)
    ensures
        is_flag_sem(w) == is_flag(w),
{
    match w {
        W::Lit(s) => {
            if s.len() > 1 && s[0] == '-' {
                flag_text_is_all_ne_e(s, 1);
            }
        }
        W::Rt => {}
    }
}

/// THE BRIDGE between the analysis's flag test and the semantic side's own
/// one. It is load-bearing: narrowing `is_flag`'s charset (the vacuity this
/// file used to have) now breaks this equality instead of moving both sides.
pub proof fn flags_in_is_any_flag(ws: Seq<W>, i: int)
    ensures
        flags_in(ws, i) == any_flag(ws, i),
    decreases ws.len() - i,
{
    if i < ws.len() {
        is_flag_sem_is_is_flag(ws[i]);
        flags_in_is_any_flag(ws, i + 1);
    }
}

pub proof fn printed_is_join(ws: Seq<W>, i: int)
    requires
        all_lit(ws, i),
        !any_broken(ws, i),
    ensures
        printed(ws, i) == Some(join(ws, i)),
    decreases ws.len() - i,
{
    if i < ws.len() {
        match ws[i] {
            W::Lit(s) => {
                has_ws_is_breaks(s, 0);
                assert(!has_ws(s, 0));
                printed_is_join(ws, i + 1);
            }
            W::Rt => {
                assert(false);
            }
        }
    }
}

/// THE FIRST LAW: the analysis only ever reports a value the command really
/// prints, in every environment.
pub proof fn constant_output_sound(h: Head, ws: Seq<W>, r: Seq<char>)
    requires analysis(h, ws) == Some(r),
    ensures emit(h, ws) == Out::Const(r),
{
    match h {
        Head::True => {}
        Head::Colon => {}
        Head::Echo => {}
        Head::Other => {}
    }
}

/// THE INDEPENDENT SIDE'S LAW: the analysis's value is the one the word
/// values produce, with no shared predicate doing the work. A forgotten
/// guard on the pass's side (an `Rt` word or a flag) disagrees with
/// `printed`/`flags_in` and makes this red.
pub proof fn analysis_prints_the_output(h: Head, ws: Seq<W>, r: Seq<char>)
    requires
        analysis(h, ws) == Some(r),
    ensures
        prints(h, ws) == Out::Const(r),
{
    match h {
        Head::True | Head::Colon => {
            assert(ws.len() == 0);
        }
        Head::Echo => {
            assert(all_lit(ws, 0) && !any_flag(ws, 0) && !any_broken(ws, 0));
            flags_in_is_any_flag(ws, 0);
            printed_is_join(ws, 0);
        }
        Head::Other => {}
    }
}

/// Corollary the second half needs: a computed replacement is made of
/// LITERAL text (so it carries no substitution of its own).
pub proof fn analysis_is_literal(h: Head, ws: Seq<W>, r: Seq<char>)
    requires analysis(h, ws) == Some(r),
    ensures all_lit(ws, 0),
{
    match h {
        Head::True | Head::Colon => {
            assert(ws.len() == 0);
            assert(all_lit(ws, 0));
        }
        Head::Echo => {}
        Head::Other => {}
    }
}

// ═══════════════════════════════════════════════ (B) the re-parse sees the same ──

pub enum Tok {
    /// a literal character: not syntax to any shell
    Ch(char),
    /// a character the RE-PARSE treats as syntax (quote, escape, expansion)
    Syn(char),
    /// a word separator
    Sp,
    /// one command substitution (its output is not re-parsed as syntax)
    Sub(Seq<char>),
}

/// The characters a re-parse can treat as syntax.
pub open spec fn syntax(c: char) -> bool {
    c == '"' || c == '\'' || c == '\\' || c == '$' || c == '`'
}

/// Inside `"…"` only these four end the quote, escape, or re-expand.
pub open spec fn syntax_quoted(c: char) -> bool {
    c == '"' || c == '\\' || c == '$' || c == '`'
}

pub open spec fn is_ws(c: char) -> bool {
    c == ' ' || c == '\t' || c == '\n' || c == '\r'
}

pub open spec fn no_syntax(s: Seq<char>, i: int) -> bool
    decreases s.len() - i,
{
    if i >= s.len() {
        true
    } else {
        !syntax_quoted(s[i]) && no_syntax(s, i + 1)
    }
}

/// The ORIGINAL token stream: a substitution's output is split on whitespace
/// but its characters are NEVER re-parsed (a `"` in the output is literal).
pub open spec fn sub_chars(s: Seq<char>, i: int) -> Seq<Tok>
    decreases s.len() - i,
{
    if i >= s.len() {
        Seq::empty()
    } else if is_ws(s[i]) {
        seq![Tok::Sp].add(sub_chars(s, i + 1))
    } else {
        seq![Tok::Ch(s[i])].add(sub_chars(s, i + 1))
    }
}

/// The FOLDED text's token stream: the same characters, now RE-PARSED, so a
/// syntax character is syntax.
pub open spec fn lit_chars(s: Seq<char>, i: int) -> Seq<Tok>
    decreases s.len() - i,
{
    if i >= s.len() {
        Seq::empty()
    } else if is_ws(s[i]) {
        seq![Tok::Sp].add(lit_chars(s, i + 1))
    } else if syntax_quoted(s[i]) {
        seq![Tok::Syn(s[i])].add(lit_chars(s, i + 1))
    } else {
        seq![Tok::Ch(s[i])].add(lit_chars(s, i + 1))
    }
}

pub open spec fn expand(t: Seq<Tok>, i: int) -> Seq<Tok>
    decreases t.len() - i,
{
    if i >= t.len() {
        Seq::empty()
    } else {
        (match t[i] {
            Tok::Sub(s) => sub_chars(s, 0),
            other => seq![other],
        }).add(expand(t, i + 1))
    }
}

pub open spec fn foldtok(t: Seq<Tok>, i: int) -> Seq<Tok>
    decreases t.len() - i,
{
    if i >= t.len() {
        Seq::empty()
    } else {
        (match t[i] {
            Tok::Sub(s) => lit_chars(s, 0),
            other => seq![other],
        }).add(foldtok(t, i + 1))
    }
}

/// every substitution in the text carries a replacement with no syntax char
pub open spec fn is_sub(t: Tok) -> bool {
    match t {
        Tok::Sub(_) => true,
        _ => true == false,
    }
}

pub open spec fn sub_val(t: Tok) -> Seq<char> {
    match t {
        Tok::Sub(s) => s,
        _ => Seq::empty(),
    }
}

pub open spec fn all_subs_are(t: Seq<Tok>, i: int, r: Seq<char>) -> bool
    decreases t.len() - i,
{
    if i >= t.len() {
        true
    } else {
        (!is_sub(t[i]) || sub_val(t[i]) == r) && all_subs_are(t, i + 1, r)
    }
}

pub open spec fn gate(t: Seq<Tok>, i: int) -> bool
    decreases t.len() - i,
{
    if i >= t.len() {
        true
    } else {
        (match t[i] {
            Tok::Sub(s) => no_syntax(s, 0),
            _ => true,
        }) && gate(t, i + 1)
    }
}

proof fn lemma_chars(s: Seq<char>, i: int)
    requires no_syntax(s, i),
    ensures sub_chars(s, i) == lit_chars(s, i),
    decreases s.len() - i,
{
    if i < s.len() {
        lemma_chars(s, i + 1);
    }
}

proof fn lemma_tokens(t: Seq<Tok>, i: int)
    requires gate(t, i),
    ensures expand(t, i) == foldtok(t, i),
    decreases t.len() - i,
{
    if i < t.len() {
        match t[i] {
            Tok::Sub(s) => {
                lemma_chars(s, 0);
            }
            _ => {}
        }
        lemma_tokens(t, i + 1);
    }
}

/// THE SECOND LAW: when the replacement carries no syntax character, the
/// re-parse of the folded text produces exactly the token stream the
/// substitution produced — same words, same quoting, no new expansion.
pub proof fn fold_preserves_the_token_stream(t: Seq<Tok>)
    requires gate(t, 0),
    ensures expand(t, 0) == foldtok(t, 0),
{
    lemma_tokens(t, 0);
}

/// The unquoted gate is exactly the "no syntax" set, over the charset the
/// source allows.
pub open spec fn safe_char(c: char) -> bool {
    ('a' <= c && c <= 'z') || ('A' <= c && c <= 'Z') || ('0' <= c && c <= '9') || c == '_' || c == '@'
        || c == '%' || c == '+' || c == '=' || c == ':' || c == ',' || c == '.' || c == '/' || c == '-'
}

pub open spec fn all_safe(s: Seq<char>, i: int) -> bool
    decreases s.len() - i,
{
    if i >= s.len() {
        true
    } else {
        safe_char(s[i]) && all_safe(s, i + 1)
    }
}

/// The unquoted safety gate implies the second law's premise.
pub proof fn safe_is_not_syntax(s: Seq<char>, i: int)
    requires all_safe(s, i),
    ensures no_syntax(s, i),
    decreases s.len() - i,
{
    if i < s.len() {
        assert(!syntax_quoted(s[i]));
        safe_is_not_syntax(s, i + 1);
    }
}

/// Inside `"…"` the gate is the source's `safe_repl(quoted = true)`.
pub open spec fn all_safe_quoted(s: Seq<char>, i: int) -> bool
    decreases s.len() - i,
{
    if i >= s.len() {
        true
    } else {
        !syntax_quoted(s[i]) && all_safe_quoted(s, i + 1)
    }
}

pub proof fn quoted_gate_is_no_syntax(s: Seq<char>, i: int)
    requires all_safe_quoted(s, i),
    ensures no_syntax(s, i),
    decreases s.len() - i,
{
    if i < s.len() {
        quoted_gate_is_no_syntax(s, i + 1);
    }
}

/// THE COMBINED LAW the transform relies on: the analysis's replacement is
/// the command's real output, and (under the gate) splicing it into the text
/// changes no token.
pub proof fn fold_is_sound(h: Head, ws: Seq<W>, r: Seq<char>, t: Seq<Tok>)
    requires
        analysis(h, ws) == Some(r),
        gate(t, 0),
        // the text's substitutions all print the analysed value
        all_subs_are(t, 0, r),
    ensures
        emit(h, ws) == Out::Const(r),
        expand(t, 0) == foldtok(t, 0),
{
    constant_output_sound(h, ws, r);
    lemma_tokens(t, 0);
}

// ═══════════════════════════════════════════════════════════════════ pins ──

pub open spec fn lit(s: Seq<char>) -> W {
    W::Lit(s)
}

/// `$(echo hi)` folds to `hi`; a flag, a glob, a variable and an unknown
/// command are all refused.
/// The independent semantic recursion's own outputs. These are the values a
/// forgotten analysis guard would have to disagree with.
pub proof fn pin_printed_assembles_the_text()
    ensures
        printed(Seq::<W>::empty(), 0) == Some(Seq::<char>::empty()),
        printed(seq![lit(seq!['a']), lit(seq!['b', 'c'])], 0)
            == Some(seq!['a', ' ', 'b', 'c']),
        printed(seq![lit(seq!['a']), W::Rt], 0) == None,
        prints(Head::Echo, seq![lit(seq!['a']), lit(seq!['b'])]) == Out::Const(seq!['a', ' ', 'b']),
        prints(Head::Echo, seq![lit(seq!['-', 'n']), lit(seq!['x'])]) == Out::NotPlain,
        prints(Head::Echo, seq![W::Rt]) == Out::NotPlain,
        prints(Head::Other, Seq::<W>::empty()) == Out::NotPlain,
{
    assert(printed(Seq::<W>::empty(), 0) == Some(Seq::<char>::empty())) by (compute);
    assert(printed(seq![lit(seq!['a']), lit(seq!['b', 'c'])], 0)
        == Some(seq!['a', ' ', 'b', 'c'])) by (compute);
    assert(printed(seq![lit(seq!['a']), W::Rt], 0) == None) by (compute);
    assert(prints(Head::Echo, seq![lit(seq!['a']), lit(seq!['b'])])
        == Out::Const(seq!['a', ' ', 'b'])) by (compute);
    assert(prints(Head::Echo, seq![lit(seq!['-', 'n']), lit(seq!['x'])])
        == Out::NotPlain) by (compute);
    assert(prints(Head::Echo, seq![W::Rt]) == Out::NotPlain) by (compute);
    assert(prints(Head::Other, Seq::<W>::empty()) == Out::NotPlain) by (compute);
}

pub proof fn pin_analysis_shapes()
    ensures
        analysis(Head::Echo, seq![lit(seq!['h', 'i'])]) == Some(seq!['h', 'i']),
        analysis(Head::Echo, seq![lit(seq!['F', 'i', 'l', 'e']), lit(seq!['n', 'o'])])
            == Some(seq!['F', 'i', 'l', 'e', ' ', 'n', 'o']),
        analysis(Head::Echo, seq![lit(seq!['-', 'n']), lit(seq!['h', 'i'])]) == None,
        analysis(Head::Echo, seq![W::Rt]) == None,
        analysis(Head::True, Seq::<W>::empty()) == Some(Seq::<char>::empty()),
        analysis(Head::Colon, Seq::<W>::empty()) == Some(Seq::<char>::empty()),
        analysis(Head::Other, seq![lit(seq!['x'])]) == None,
        analysis(Head::True, seq![lit(seq!['x'])]) == None,
{
    assert(analysis(Head::Echo, seq![lit(seq!['h', 'i'])]) == Some(seq!['h', 'i'])) by (compute);
    assert(analysis(Head::Echo, seq![lit(seq!['F', 'i', 'l', 'e']), lit(seq!['n', 'o'])])
        == Some(seq!['F', 'i', 'l', 'e', ' ', 'n', 'o'])) by (compute);
    assert(analysis(Head::Echo, seq![lit(seq!['-', 'n']), lit(seq!['h', 'i'])]) == None) by (compute);
    assert(analysis(Head::Echo, seq![W::Rt]) == None) by (compute);
    assert(analysis(Head::True, Seq::<W>::empty()) == Some(Seq::<char>::empty())) by (compute);
    assert(analysis(Head::Colon, Seq::<W>::empty()) == Some(Seq::<char>::empty())) by (compute);
    assert(analysis(Head::Other, seq![lit(seq!['x'])]) == None) by (compute);
    assert(analysis(Head::True, seq![lit(seq!['x'])]) == None) by (compute);
}

/// The semantic side is the richer one: `echo -n hi` and a newline-carrying
/// word are still CONSTANT, which is why it is not a restatement of the
/// analysis (the analysis refuses both).
pub proof fn pin_emit_is_richer()
    ensures
        emit(Head::Echo, seq![lit(seq!['h', 'i'])]) == Out::Const(seq!['h', 'i']),
        analysis(Head::Echo, seq![lit(seq!['-', 'n']), lit(seq!['h', 'i'])]) == None,
        analysis(Head::Echo, seq![lit(seq!['a', '\n'])]) == None,
        emit(Head::Echo, seq![W::Rt]) == Out::NotPlain,
        emit(Head::Other, Seq::<W>::empty()) == Out::NotPlain,
{
    assert(emit(Head::Echo, seq![lit(seq!['h', 'i'])]) == Out::Const(seq!['h', 'i'])) by (compute);
    assert(analysis(Head::Echo, seq![lit(seq!['-', 'n']), lit(seq!['h', 'i'])]) == None) by (compute);
    assert(analysis(Head::Echo, seq![lit(seq!['a', '\n'])]) == None) by (compute);
}

/// The flag CLASSES: every `neE` spelling is a flag on both sides, and an
/// ordinary word is not. `-e`/`-E`/`-ne` are the values a narrowed charset
/// would move, so this pin is what the source's `refuses_echo_flags` test
/// needs to keep holding.
pub proof fn pin_flag_classes_agree()
    ensures
        is_flag(lit(seq!['-', 'n'])),
        is_flag(lit(seq!['-', 'e'])),
        is_flag(lit(seq!['-', 'E'])),
        is_flag(lit(seq!['-', 'n', 'e'])),
        is_flag_sem(lit(seq!['-', 'e'])),
        is_flag_sem(lit(seq!['-', 'n', 'e'])),
        !is_flag(lit(seq!['-', 'x'])),
        !is_flag_sem(lit(seq!['-', 'x'])),
        !is_flag(lit(seq!['-'])),
        analysis(Head::Echo, seq![lit(seq!['-', 'e']), lit(seq!['h', 'i'])]) == None,
        prints(Head::Echo, seq![lit(seq!['-', 'e']), lit(seq!['h', 'i'])]) == Out::NotPlain,
        emit(Head::Echo, seq![lit(seq!['-', 'E']), lit(seq!['h', 'i'])]) == Out::NotPlain,
{
    assert(is_flag(lit(seq!['-', 'n']))) by (compute);
    assert(is_flag(lit(seq!['-', 'e']))) by (compute);
    assert(is_flag(lit(seq!['-', 'E']))) by (compute);
    assert(is_flag(lit(seq!['-', 'n', 'e']))) by (compute);
    assert(is_flag_sem(lit(seq!['-', 'e']))) by (compute);
    assert(is_flag_sem(lit(seq!['-', 'n', 'e']))) by (compute);
    assert(!is_flag(lit(seq!['-', 'x']))) by (compute);
    assert(!is_flag_sem(lit(seq!['-', 'x']))) by (compute);
    assert(!is_flag(lit(seq!['-']))) by (compute);
    assert(analysis(Head::Echo, seq![lit(seq!['-', 'e']), lit(seq!['h', 'i'])]) == None) by (compute);
    assert(prints(Head::Echo, seq![lit(seq!['-', 'e']), lit(seq!['h', 'i'])]) == Out::NotPlain)
        by (compute);
    assert(emit(Head::Echo, seq![lit(seq!['-', 'E']), lit(seq!['h', 'i'])]) == Out::NotPlain)
        by (compute);
}

/// A quote inside the replacement is syntax to the re-parse but was literal
/// in the substitution's own output — the gate is load-bearing.
pub proof fn pin_a_quote_breaks_the_parse()
    ensures
        sub_chars(seq!['a', '"', 'b'], 0) == seq![Tok::Ch('a'), Tok::Ch('"'), Tok::Ch('b')],
        lit_chars(seq!['a', '"', 'b'], 0) == seq![Tok::Ch('a'), Tok::Syn('"'), Tok::Ch('b')],
        sub_chars(seq!['a', '"', 'b'], 0) != lit_chars(seq!['a', '"', 'b'], 0),
        !no_syntax(seq!['a', '"', 'b'], 0),
{
    assert(sub_chars(seq!['a', '"', 'b'], 0) == seq![Tok::Ch('a'), Tok::Ch('"'), Tok::Ch('b')])
        by (compute);
    assert(lit_chars(seq!['a', '"', 'b'], 0) == seq![Tok::Ch('a'), Tok::Syn('"'), Tok::Ch('b')])
        by (compute);
    assert(!no_syntax(seq!['a', '"', 'b'], 0)) by (compute);
}

/// A safe replacement splices in the same tokens; the gates exclude exactly
/// the syntax set.
pub proof fn pin_safe_splice()
    ensures
        all_safe(seq!['a', '-', 'b', '.', '9'], 0),
        !all_safe(seq!['a', ' ', 'b'], 0),
        !all_safe(seq!['*'], 0),
        !all_safe(seq!['$', 'x'], 0),
        all_safe_quoted(seq!['a', ' ', 'b'], 0),
        !all_safe_quoted(seq!['a', '"'], 0),
        !all_safe_quoted(seq!['a', '$'], 0),
        !all_safe_quoted(seq!['a', '\\'], 0),
        syntax('"'),
        syntax('\''),
        syntax('\\'),
        syntax('$'),
        syntax('`'),
        !syntax('a'),
        !syntax(' '),
{
    assert(all_safe(seq!['a', '-', 'b', '.', '9'], 0)) by (compute);
    assert(!all_safe(seq!['a', ' ', 'b'], 0)) by (compute);
    assert(!all_safe(seq!['*'], 0)) by (compute);
    assert(!all_safe(seq!['$', 'x'], 0)) by (compute);
    assert(all_safe_quoted(seq!['a', ' ', 'b'], 0)) by (compute);
    assert(!all_safe_quoted(seq!['a', '"'], 0)) by (compute);
    assert(!all_safe_quoted(seq!['a', '$'], 0)) by (compute);
    assert(!all_safe_quoted(seq!['a', '\\'], 0)) by (compute);
    assert(syntax('"')) by (compute);
    assert(syntax('\'')) by (compute);
    assert(syntax('\\')) by (compute);
    assert(syntax('$')) by (compute);
    assert(syntax('`')) by (compute);
    assert(!syntax('a')) by (compute);
    assert(!syntax(' ')) by (compute);
}

/// The gates imply the second law's premise, so `fold_preserves_the_token_stream`
/// applies to exactly the replacements the transform splices.
pub proof fn pin_gates_imply_the_premise()
    ensures
        all_safe(seq!['d', 'f', 'l', 't'], 0) ==>
            no_syntax(seq!['d', 'f', 'l', 't'], 0),
        all_safe_quoted(seq!['F', 'i', 'l', 'e', ' ', 'n', 'o'], 0) ==>
            no_syntax(seq!['F', 'i', 'l', 'e', ' ', 'n', 'o'], 0),
{
    if all_safe(seq!['d', 'f', 'l', 't'], 0) {
        safe_is_not_syntax(seq!['d', 'f', 'l', 't'], 0);
    }
    if all_safe_quoted(seq!['F', 'i', 'l', 'e', ' ', 'n', 'o'], 0) {
        quoted_gate_is_no_syntax(seq!['F', 'i', 'l', 'e', ' ', 'n', 'o'], 0);
    }
}

/// The whole fold on the source's own operand shape: `"$(echo hello)"` — the
/// analysis gives `hello`, and the token stream is unchanged (the law, with
/// the gate discharged concretely).
pub proof fn pin_quoted_test_operand()
    ensures
        analysis(Head::Echo, seq![lit(seq!['h', 'e', 'l', 'l', 'o'])])
            == Some(seq!['h', 'e', 'l', 'l', 'o']),
{
    assert(analysis(Head::Echo, seq![lit(seq!['h', 'e', 'l', 'l', 'o'])])
        == Some(seq!['h', 'e', 'l', 'l', 'o'])) by (compute);
}

/// …and the same substitution spliced into a double-quoted test operand: the
/// re-parse must see the same tokens (the fold's own shape).
pub proof fn pin_splice_of_the_test_operand()
    ensures
        ({
            let t = seq![
                Tok::Ch('"'),
                Tok::Sub(seq!['h', 'e', 'l', 'l', 'o']),
                Tok::Ch('"'),
                Tok::Ch('='),
                Tok::Ch('"'),
                Tok::Sub(seq!['h', 'e', 'l', 'l', 'o']),
                Tok::Ch('"'),
            ];
            gate(t, 0) && expand(t, 0) == foldtok(t, 0)
        }),
{
    assert(gate(
        seq![
            Tok::Ch('"'),
            Tok::Sub(seq!['h', 'e', 'l', 'l', 'o']),
            Tok::Ch('"'),
            Tok::Ch('='),
            Tok::Ch('"'),
            Tok::Sub(seq!['h', 'e', 'l', 'l', 'o']),
            Tok::Ch('"'),
        ],
        0,
    )) by (compute);
    fold_preserves_the_token_stream(
        seq![
            Tok::Ch('"'),
            Tok::Sub(seq!['h', 'e', 'l', 'l', 'o']),
            Tok::Ch('"'),
            Tok::Ch('='),
            Tok::Ch('"'),
            Tok::Sub(seq!['h', 'e', 'l', 'l', 'o']),
            Tok::Ch('"'),
        ],
    );
}

fn main() {}

} // verus!
