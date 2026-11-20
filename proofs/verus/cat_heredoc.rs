// Standalone model of `cat_heredoc`
// (sh2perl/src/transforms/cat_heredoc.rs).
//
// `cat <<'EOF' … EOF` in statement position with a literal body is a pure
// print: the body bytes, exactly once. The pass rewrites the
// `Redirect { inner: [exec("cat")], redirects: [{fd: 0, mode: heredoc}] }`
// into an `Output` of the body text, so no backend needs a fork/exec.
//
// ## The obligation
//
//   lift_matches_bash: when the pass fires, the bytes it prints are exactly
//   the bytes bash's `cat` would print, for every environment — including
//   the interpolating-but-expansion-free case (a `$`-free body expands to
//   itself).
//
//   expansion_guard_is_load_bearing: without the `interpolate && body has
//   `$`/backtick/backslash` refusal, the emitted text would be the RAW body
//   where bash expands it — a concrete witness, kept as an obligation.
//
//   shape_refusals: the other guards (one redirect, fd 0, `heredoc` not
//   `heredoc-tabs`, no operands, exactly one inner `cat`) are pinned as
//   facts about `lifts`, so dropping one is red even though it cannot change
//   the bytes theorem.
//
// ## Scope (read before quoting it)
//
// - Model, not code: the body is a sequence of `Piece`s — `Lit(c)` is one
//   literal intacter, `Var(c)` is a `$c`-style expansion (the abstract form
//   of `$`/backtick/backslash hazards). `expand(p, env)` substitutes the
//   environment's intacter for each `Var`.
// - The diagnostic/`newline` flag is out of scope: the source leaves
//   `newline: false` deliberately for `output-line-fold` to consume later;
//   the model compares BYTES, not line-arm choice.
// - `<<-` tab stripping is refused by the pass and is not modelled (the
//   parser owns it), matching the source's scope note.
//
// Verus 0.2026.10.18.7d82994, rust 1.99.1:
//   verus --triggers-mode silent --rlimit 40 cat_heredoc.rs
//   -> verification results:: 13 verified, 0 errors
//
// Mutants (checked, measured on copies):
//   - dropping the expansion guard (`!interpolate || !has_expansion`):
//     10 verified, 3 errors (the bytes theorem and the refusal pin both fail).
//   - dropping the `mode == heredoc` check: 12 verified, 1 error (the shape
//     pin — the bytes theorem cannot see it).
// Restored and re-checked at 13 verified, 0 errors.
use vstd::prelude::*;

verus! {

pub enum Piece {
    Lit(int),
    // A `$` expansion (or backtick/backslash hazard): `text` renders it as
    // `$c`, `expand` replaces it with the environment's intacter.
    Var(int),
}

pub type Bytes = Seq<int>;
pub type Env = Map<int, int>;

pub open spec fn text(p: Seq<Piece>) -> Bytes
    decreases p.len(),
{
    if p.len() == 0 {
        Seq::empty()
    } else {
        match p[0] {
            Piece::Lit(c) => seq![c] + text(p.subrange(1, p.len() as int)),
            Piece::Var(c) => seq![36, c] + text(p.subrange(1, p.len() as int)),
        }
    }
}

pub open spec fn has_expansion(p: Seq<Piece>) -> bool
    decreases p.len(),
{
    if p.len() == 0 {
        false
    } else {
        match p[0] {
            Piece::Var(_) => true,
            Piece::Lit(_) => has_expansion(p.subrange(1, p.len() as int)),
        }
    }
}

pub open spec fn expand(p: Seq<Piece>, env: Env) -> Bytes
    decreases p.len(),
{
    if p.len() == 0 {
        Seq::empty()
    } else {
        match p[0] {
            Piece::Lit(c) => seq![c] + expand(p.subrange(1, p.len() as int), env),
            Piece::Var(c) => {
                let v = if env.contains_key(c) { env[c] } else { 63 };
                seq![v] + expand(p.subrange(1, p.len() as int), env)
            }
        }
    }
}

// An expansion-free body is its own text under every environment.
proof fn expansion_free_is_text(p: Seq<Piece>, env: Env)
    requires !has_expansion(p),
    ensures expand(p, env) == text(p),
    decreases p.len(),
{
    reveal_with_fuel(text, 10);
    reveal_with_fuel(expand, 10);
    reveal_with_fuel(has_expansion, 10);
    if p.len() > 0 {
        expansion_free_is_text(p.subrange(1, p.len() as int), env);
    }
}

// ---- the heredoc shape ----

pub struct Hd {
    /// Exactly one inner `exec`/`builtin` call of `cat` with no operands.
    pub one_cat: bool,
    pub n_redirects: nat,
    pub fd: int,
    /// 0 = `heredoc`, 1 = `heredoc-tabs`, 2 = anything else.
    pub mode: nat,
    pub interpolate: bool,
    pub body: Seq<Piece>,
}

// The pass's guard, as a predicate over the shape. Every conjunct is one of
// the source's checks.
pub open spec fn lifts(h: Hd) -> bool {
    h.one_cat
        && h.n_redirects == 1
        && h.fd == 0
        && h.mode == 0
        && (!h.interpolate || !has_expansion(h.body))
}

// `cat` on a heredoc prints exactly the body bash produced.
pub open spec fn bash_bytes(h: Hd, env: Env) -> Bytes {
    if h.interpolate {
        expand(h.body, env)
    } else {
        text(h.body)
    }
}

// The `Output` the pass emits prints the body's text.
pub open spec fn emitted(h: Hd) -> Bytes {
    text(h.body)
}

// THE OBLIGATION: whenever the pass fires, it prints bash's bytes.
proof fn lift_matches_bash(h: Hd, env: Env)
    requires lifts(h),
    ensures emitted(h) == bash_bytes(h, env),
{
    reveal_with_fuel(text, 10);
    reveal_with_fuel(expand, 10);
    reveal_with_fuel(has_expansion, 10);
    if h.interpolate {
        // the guard forced an expansion-free body
        expansion_free_is_text(h.body, env);
    } else {
        // a quoted delimiter suppresses expansion
    }
}

// ---- the guard that keeps the theorem true ----

// The naive rewrite (lift regardless of `$`/backtick/backslash in an
// interpolating body) prints text bash would have expanded.
pub open spec fn naive_lifts(h: Hd) -> bool {
    h.one_cat && h.n_redirects == 1 && h.fd == 0 && h.mode == 0
}

pub open spec fn dollar_body() -> Seq<Piece> {
    seq![Piece::Var(120), Piece::Lit(33)]
}

pub open spec fn dollar_heredoc() -> Hd {
    Hd {
        one_cat: true,
        n_redirects: 1,
        fd: 0,
        mode: 0,
        interpolate: true,
        body: dollar_body(),
    }
}

proof fn expansion_guard_is_load_bearing()
    ensures ({
        let env = Map::<int, int>::empty().insert(120, 57);
        let h = dollar_heredoc();
        // the shipped guard refuses it...
        !lifts(h)
            // ...and the naive rewrite would print `$x!` where bash prints
            // `9!`
            && naive_lifts(h)
            && emitted(h) != bash_bytes(h, env)
    }),
{
    reveal_with_fuel(text, 10);
    reveal_with_fuel(expand, 10);
    let env = Map::<int, int>::empty().insert(120, 57);
    let h = dollar_heredoc();
    assert(emitted(h) == seq![36, 120, 33]);
    assert(bash_bytes(h, env) == seq![57, 33]);
    assert(emitted(h).len() != bash_bytes(h, env).len());
}

// The quoted form of the same body is literal text: `$x!` stays `$x!`.
proof fn quoted_dollar_body_is_literal()
    ensures ({
        let env = Map::<int, int>::empty().insert(120, 57);
        let h = Hd {
            one_cat: true,
            n_redirects: 1,
            fd: 0,
            mode: 0,
            interpolate: false,
            body: dollar_body(),
        };
        lifts(h) && emitted(h) == bash_bytes(h, env) && emitted(h) == seq![36, 120, 33]
    }),
{
    reveal_with_fuel(text, 10);
}

// ---- pins: the source's tests ----

// quoted_literal_heredoc_lifts_to_output: several bodies, bytes verbatim.
proof fn pin_quoted_literal_heredoc_lifts()
    ensures ({
        let h0 = Hd { one_cat: true, n_redirects: 1, fd: 0, mode: 0, interpolate: false,
            body: seq![Piece::Lit(104), Piece::Lit(105), Piece::Lit(10)] };
        let h1 = Hd { one_cat: true, n_redirects: 1, fd: 0, mode: 0, interpolate: false,
            body: seq![Piece::Lit(110), Piece::Lit(111)] };
        let h2 = Hd { one_cat: true, n_redirects: 1, fd: 0, mode: 0, interpolate: false,
            body: seq![Piece::Lit(10), Piece::Lit(10)] };
        lifts(h0) && emitted(h0) == seq![104, 105, 10]
            && lifts(h1) && emitted(h1) == seq![110, 111]
            && lifts(h2) && emitted(h2) == seq![10, 10]
    }),
{
    reveal_with_fuel(text, 10);
}

// unquoted_body_without_expansion_lifts: interpolation is a no-op when the
// body names nothing.
proof fn pin_unquoted_body_without_expansion_lifts()
    ensures ({
        let h = Hd {
            one_cat: true,
            n_redirects: 1,
            fd: 0,
            mode: 0,
            interpolate: true,
            body: seq![Piece::Lit(112), Piece::Lit(33)],
        };
        lifts(h) && emitted(h) == bash_bytes(h, Map::<int, int>::empty())
    }),
{
    reveal_with_fuel(text, 10);
    reveal_with_fuel(expand, 10);
    reveal_with_fuel(has_expansion, 10);
}

// interpolating_body_with_expansion_refuses: every hazard shape, untouched.
proof fn pin_interpolating_body_with_expansion_refuses()
    ensures ({
        let h = dollar_heredoc();
        !lifts(h)
    }),
{
}

// a_quoted_body_keeps_dollar_text (the `cost: $5` case).
proof fn pin_quoted_body_keeps_dollar_text()
    ensures ({
        let h = Hd {
            one_cat: true,
            n_redirects: 1,
            fd: 0,
            mode: 0,
            interpolate: false,
            body: seq![Piece::Lit(99), Piece::Var(53)],
        };
        lifts(h) && emitted(h) == seq![99, 36, 53]
    }),
{
    reveal_with_fuel(text, 10);
}

// heredoc_tabs_and_other_fds_refuse / cat_with_operands_and_extra_redirects.
proof fn pin_shape_refusals()
    ensures ({
        let b = seq![Piece::Lit(120)];
        let base = Hd { one_cat: true, n_redirects: 1, fd: 0, mode: 0, interpolate: false, body: b };
        let tabs = Hd { one_cat: true, n_redirects: 1, fd: 0, mode: 1, interpolate: false, body: b };
        let fd1 = Hd { one_cat: true, n_redirects: 1, fd: 1, mode: 0, interpolate: false, body: b };
        let two = Hd { one_cat: true, n_redirects: 2, fd: 0, mode: 0, interpolate: false, body: b };
        let operand = Hd { one_cat: false, n_redirects: 1, fd: 0, mode: 0, interpolate: false, body: b };
        lifts(base) && !lifts(tabs) && !lifts(fd1) && !lifts(two) && !lifts(operand)
    }),
{
}

fn main() {}
}
