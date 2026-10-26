//! affix-strip-fold — `${v#lit}` is `strip_prefix(lit)`: one node, not a
//! glob engine.
//!
//! name: affix-strip-fold
//! prereqs: [none — param("#"/"##"/"%"/"%%", …) comes straight from the
//!   parser's parameter-expansion lowering (words.rs → ast_words.rs →
//!   shir.rs, which spells the operator as "#" / "##" / "%" / "%%" with
//!   the pattern in the 3rd slot)]
//! invariant: param(OP, NAME, PAT) folds to AffixStrip(text=param("",NAME),
//!   pattern=PAT, prefix) ONLY when OP ∈ {"#","##","%","%%"}, NAME is a
//!   plain identifier ([A-Za-z_][A-Za-z0-9_]*) or an array-element read
//!   with a bare identifier/number subscript (`args[i]`, `args[0]` — see
//!   `is_text_operand_name`), and PAT is either (a) a literal string
//!   carrying NO glob magic (`*`, `?`, `[`, `\`) → AffixStrip, or (b) the
//!   star-form family (see below): exactly ONE `*` pinned at either end
//!   with a magic-free literal body (possibly empty) → the four
//!   find()/rfind() identities fold to the CutAt node and the two
//!   strip-compatible shapes fold to AffixStrip — each gated on THAT
//!   replacement's own accepting set (AffixStrip: rust, java; CutAt:
//!   rust), and the target backend must have a verified render arm for
//!   the node it produces.
//!   For a magic-free literal, bash's SHORTEST and LONGEST match are
//!   identical (a literal matches exactly once, at the very front for
//!   `#`/`##` and at the very end for `%`/`%%`), so `#`≡`##` and `%`≡`%%`
//!   and each reduces to removing AT MOST ONE leading/trailing occurrence
//!   — which is precisely AffixStrip's semantics (`strip_prefix`/
//!   `strip_suffix` in Rust, `TrimPrefix`/`TrimSuffix` in Go,
//!   `startsWith`/`endsWith`+`substring` in Java). Everything else is
//!   refuse-on-doubt: a pattern with glob magic (the engine is REQUIRED
//!   then), a non-literal or interpolated pattern, a NAME that is neither
//!   a plain read nor a bare-subscript element read (`arr[@]`, `arr[*]`,
//!   `arr[x+1]`, `@`, `*`, `!p`, `#n` — whole-array joins and
//!   positional/indirect spellings mean something else), the `/`/`//`
//!   substitute ops, and any
//!   target without a VERIFIED AffixStrip render (estree/perl/python
//!   already inline literals optimally; c/sh/zig have no AffixStrip arm;
//!   go's Ext path emits a `sh2.AffixStrip(...)` stub rather than routing
//!   to the shared handler, so it is refused until that backend accepts
//!   the node).
//! scope: offered to rust, java (AffixStrip) and rust, c (CutAt) —
//!   target-gated, end-to-end verified. estree/perl/python keep their own
//!   inline literal render (already idiomatic); sh/zig/fish/zsh/glsl/go
//!   refuse (no verified render arm); c refuses the AffixStrip branch
//!   only, having no AffixStrip arm.
//!   The star-form CutAt branch is offered to rust AND c (c renders the
//!   same single find/rfind cut into a vbuf — that arm is what retires
//!   `_sh_strippre`/`fnmatch` for MIMEcroft's tokenizer, ~14% of the
//!   run's executed lines); other backends accept by adding an arm and
//!   their target (the FieldGrab offer pattern). The Affix-star shapes
//!   (`#lit*`, `%*lit`) reuse the existing AffixStrip arms, so java
//!   accepts those two as before and c still refuses them.
//! updates: none (first offer)
//!
//! ## Need
//! The parser lowers `${v#lit}` to a `param("#", v, lit)` CALL, and each
//! backend then re-derives — at every render — that a literal pattern
//! needs no glob engine:
//!   * Rust spins up `__sh_strippre`, whose helper table pulls in
//!     `__sh_fnmatch` (the full glob engine: `Vec<char>`, prefix/suffix
//!     enumeration, `fnmatch`-style matching) just to print `${v#git-}`
//!     where `v.strip_prefix("git-")` would do;
//!   * Go emits `pStrip` → `re.FindStringIndex` (the regex engine) for the
//!     same literal;
//!   * Java emits `shStripShortestPrefix` → its own glob matcher.
//! Whether a pattern is magic-free is a PROGRAM property, not a backend
//! detail — so the rewrite belongs in the shared shIR→shIR channel: one
//! fold retires the glob/regex helper (and its definition) for every
//! accepting backend at once, the marketplace §11 shape. Measured on the
//! emitted corpus: 14 literal strip sites across 9 files, of which 7 files
//! use ONLY literal strips (folding retires `__sh_fnmatch` entirely there).
//!
//! ## Star-form identities (`lit*` / `*lit`)
//!
//! The single-star family is the dirname/basename bread-and-butter of
//! shell scripts (`${path%/*}`, `${path##*/}`, `${s#*:}`) — measured at
//! ~35 sites across the corpus — and each identity follows directly
//! from glob semantics: `*` matches the empty string, shortest-vs-longest
//! picks the cut point, and a no-match strip leaves the value unchanged.
//! All eight forms were verified against bash (match / no-match /
//! empty-body / multi-byte-literal cases) before the fold was written:
//!
//! | OP   | PAT shape | replacement                       |
//! |------|-----------|-----------------------------------|
//! | `#`  | `lit*`    | AffixStrip(prefix=true,  lit) — the shortest prefix matching `lit·X` is `lit` itself, so this IS `${v#lit}` |
//! | `%`  | `*lit`    | AffixStrip(prefix=false, lit) — symmetric: shortest suffix matching `X·lit` is `lit` itself |
//! | `%`  | `lit*`    | CutAt(head,  !first) — keep before the LAST occurrence  (dirname) |
//! | `%%` | `lit*`    | CutAt(head,  first)  — keep before the FIRST occurrence |
//! | `#`  | `*lit`    | CutAt(!head, first)  — keep after the FIRST occurrence |
//! | `##` | `*lit`    | CutAt(!head, !first) — keep after the LAST occurrence  (basename) |
//! | `##` | `lit*`    | REFUSE: `""` iff starts-with, else the value — no node expresses the conditional yet |
//! | `%%` | `*lit`    | `""` literal when the body is empty (pattern `*` — the longest suffix matching `*` is the whole value); REFUSE otherwise (`""` iff ends-with) |
//!
//! REFUSE > GUESS: a second star, a middle star (`a*b` — the cut point
//! needs a scan), `?`/`[`/`\` anywhere in the body (the glob match then
//! diverges from a plain find/rfind), and everything the literal branch
//! already refused.
//!
//! ## Placement
//! Registered in `transforms.rs` (SH2_TRANSFORMS-gated, default ON —
//! empty = all). A pure local substitution of an already-lowered node:
//! the var READ is preserved (AffixStrip.text is param("",NAME), the same
//! plain read every analysis already keys), so nothing that counted the
//! slice-with-pattern afterwards is stranded.

use crate::ir::{IrExpr, IrStmt, StrStyle};
use crate::shir_nodes::{AffixStrip, CutAt};

/// Apply the transform. Returns whether anything changed.
/// Signature matches `transforms::TransformFn` (`fn(&mut Vec<IrStmt>)`).
pub fn transform(stmts: &mut Vec<IrStmt>) -> bool {
    // Target gate: only the backends whose literal-strip render currently
    // drags a glob/regex helper AND that carry a verified arm for the node
    // the matching branch produces — the UNION of the two per-replacement
    // accepting sets, because each branch is gated on its own node below
    // (AffixStrip → fold_target_ok, CutAt → cut_target_ok): c accepts only
    // the CutAt branch, java only the AffixStrip one. Unset target (the
    // shir embed path / non-CLI ingress) refuses either — the fold is a
    // render-quality optimisation, never a semantic requirement.
    if !fold_target_ok() && !cut_target_ok() {
        return false;
    }
    let mut c = false;
    for st in stmts.iter_mut() {
        c |= stmt(st);
    }
    c
}

/// Targets whose AffixStrip render is VERIFIED end-to-end (the backend
/// routes `IrExpr::Ext` to a correct native arm) AND whose current
/// `param("#"/"##"/"%"/"%%")` render is heavier than it (a glob helper
/// for a magic-free literal):
///   * rust — now renders `strip_prefix`/`strip_suffix` (this change),
///     retiring `__sh_strippre`/`__sh_fnmatch` for literal patterns;
///   * java — routes Ext through render_ext_expr → startsWith/substring;
///   * estree/perl/python already inline literals natively
///     (startsWith/slice, s{}//r, removeprefix) — folding is neutral, so
///     they refuse (no shape churn for no gain).
/// go is NOT accepted yet: its `Ext` path emits a `sh2.AffixStrip(...)`
/// STUB (not a refusal) because the Go backend doesn't route Ext to the
/// shared handler — folding there would emit an unrunnable call. c/sh/zig
/// have no AffixStrip arm (loud refusal) and stay out.
fn fold_target_ok() -> bool {
    match crate::transforms::target() {
        // c is NOT here: it has no AffixStrip arm, only the CutAt one
        // (cut_target_ok), so an Affix branch must still refuse on c.
        Some(t) => matches!(t.as_str(), "rs" | "rust" | "java"),
        None => false,
    }
}

fn stmt(st: &mut IrStmt) -> bool {
    match st {
        IrStmt::Assign { expr: rhs, .. } => expr(rhs),
        IrStmt::Declare { init: Some(e), .. } => expr(e),
        IrStmt::Expr(e) => expr(e),
        IrStmt::Output { value, .. } => expr(value),
        IrStmt::Return(Some(e)) | IrStmt::Exit(Some(e)) => expr(e),
        IrStmt::If { cond, then, elsifs, else_, .. } => {
            let mut c = expr(cond);
            c |= transform(then);
            for (ec, eb) in elsifs.iter_mut() {
                c |= expr(ec);
                c |= transform(eb);
            }
            c |= transform(else_);
            c
        }
        IrStmt::While { cond, body } | IrStmt::DoWhile { cond, body, .. } => {
            expr(cond) | transform(body)
        }
        IrStmt::For { iter, body, .. } => expr(iter) | transform(body),
        IrStmt::ForInit { init, cond, step, body } => {
            let mut c = transform(init);
            c |= expr(cond);
            for s in step.iter_mut() {
                c |= stmt(s);
            }
            c |= transform(body);
            c
        }
        IrStmt::Function { body, named_blocks, .. } => {
            let mut c = transform(body);
            for (_, b) in named_blocks.iter_mut() {
                c |= transform(b);
            }
            c
        }
        IrStmt::Block(b) | IrStmt::Subshell(b) | IrStmt::Background(b) => transform(b),
        IrStmt::Redirect { inner, .. } => transform(inner),
        IrStmt::Pipeline { stages, .. } => {
            let mut c = false;
            for s in stages.iter_mut() {
                c |= transform(s);
            }
            c
        }
        IrStmt::Try { body, excepts, else_body, finally_body } => {
            let mut c = transform(body);
            for e in excepts.iter_mut() {
                c |= transform(&mut e.body);
            }
            c |= transform(else_body);
            c |= transform(finally_body);
            c
        }
        IrStmt::Case { discriminant, clauses } => {
            let mut c = expr(discriminant);
            for cl in clauses.iter_mut() {
                c |= transform(&mut cl.body);
            }
            c
        }
        IrStmt::WriteFile { path, content, .. } => expr(path) | expr(content),
        IrStmt::DeclareArray { elements, .. } => {
            let mut c = false;
            for e in elements.iter_mut() {
                c |= expr(e);
            }
            c
        }
        _ => false,
    }
}

fn expr(e: &mut IrExpr) -> bool {
    let mut c = match e {
        IrExpr::Index { key, .. } => expr(key),
        IrExpr::BinOp { lhs, rhs, .. } => expr(lhs) | expr(rhs),
        IrExpr::Ternary { cond, then, else_ } => expr(cond) | expr(then) | expr(else_),
        IrExpr::DefinedOr { expr: x, default } => expr(x) | expr(default),
        IrExpr::Interpolate(parts) => {
            let mut c = false;
            for p in parts.iter_mut() {
                if let crate::ir::InterpPart::Expr(x) = p {
                    c |= expr(x);
                }
            }
            c
        }
        IrExpr::Array(items) => {
            let mut c = false;
            for i in items.iter_mut() {
                c |= expr(i);
            }
            c
        }
        IrExpr::Call { args, .. } => {
            let mut c = false;
            for a in args.iter_mut() {
                c |= expr(a);
            }
            c
        }
        IrExpr::MethodCall { obj, args, .. } => {
            let mut c = expr(obj);
            for a in args.iter_mut() {
                c |= expr(a);
            }
            c
        }
        IrExpr::Capture { expr: x, .. } => expr(x),
        IrExpr::Arrow(body) => transform(body),
        IrExpr::Lambda { body, .. } => transform(body),
        IrExpr::ArrayComp { iter, elem, cond, .. } => {
            let mut c = expr(iter) | expr(elem);
            if let Some(cd) = cond.as_mut() {
                c |= expr(cd);
            }
            c
        }
        IrExpr::Object(vals) => {
            let mut c = false;
            for (_, v) in vals.iter_mut() {
                c |= expr(v);
            }
            c
        }
        _ => false,
    };
    c |= fold(e);
    c
}

/// A name that is just an identifier can only be a plain parameter
/// reference — anything else (`arr[i]`, `arr[@]`, `!p`, `#n`, `@`, `*`,
/// `1`, a dotted/bracketed name) fails THIS gate — the fold's text
/// operand additionally accepts a bare-subscript element read through
/// `is_text_operand_name`; the rest keep their strip spelling, because
/// the operand then means a positional / indirect / whole-array value,
/// not one scalar string.
pub(crate) fn is_plain_ident(n: &str) -> bool {
    let mut cs = n.chars();
    match cs.next() {
        Some(c) if c.is_ascii_alphabetic() || c == '_' => {
            cs.all(|c| c.is_ascii_alphanumeric() || c == '_')
        }
        _ => false,
    }
}

/// A magic-free literal pattern: no glob metacharacter (`*`, `?`, `[`)
/// and no backslash escape. Such a pattern matches exactly one string, so
/// shortest ≡ longest and the strip removes at most ONE occurrence.
pub(crate) fn is_literal_pattern(p: &str) -> bool {
    !p.is_empty() && !p.contains('*') && !p.contains('?') && !p.contains('[') && !p.contains('\\')
}

/// The NAME slot accepted as the strip's TEXT operand: a plain scalar
/// read, or an array-ELEMENT read with a bare identifier/number
/// subscript (`args[i]`, `args[0]`). Both spellings read ONE string
/// value, and both accepting backends render `param("", NAME)` for them
/// natively — rust's `param_str` falls through its bracket branch to
/// `array_index_name` (the same `var_expr` the strip arm uses), java
/// documents `param("", "arr[idx]")` as the arrival shape of
/// `${arr[idx]}` — so rebuilding the text as the plain read renders
/// exactly today's operand. Whole-array spellings (`arr[@]`, `arr[*]` —
/// a JOINED read java resolves as the single element key), operator
/// subscripts (`arr[x+1]`), and the positional/indirect/dotted
/// spellings stay refused: REFUSE > GUESS.
pub(crate) fn is_text_operand_name(n: &str) -> bool {
    if is_plain_ident(n) {
        return true;
    }
    let Some(b) = n.find('[') else { return false; };
    if !n.ends_with(']') {
        return false;
    }
    let inner = &n[b + 1..n.len() - 1];
    is_plain_ident(&n[..b])
        && !inner.is_empty()
        && inner.chars().all(|c| c.is_ascii_alphanumeric() || c == '_')
}

/// The fold's two replacement shapes (classified BEFORE the target gate
/// fires, because each has its own accepting set).
enum Replacement {
    /// Strip at most ONE literal occurrence — the existing AffixStrip.
    Affix { prefix: bool },
    /// Cut at ONE occurrence of the literal delimiter — the star-form
    /// CutAt node: `head` keeps the head side of the cut (else the tail),
    /// `first` cuts at the first occurrence (else the last).
    Cut { head: bool, first: bool },
}

/// Fold one node: `param(OP, NAME, PAT)` → AffixStrip (magic-free
/// literal) or CutAt (single-star `lit*`/`*lit` body) over the plain read
/// `param("", NAME)`.
fn fold(e: &mut IrExpr) -> bool {
    let IrExpr::Call { func, args } = e else {
        return false;
    };
    if func != "param" {
        return false;
    }
    // The parser's PATH-strip spellings: `${p%/*}` arrives as the 2-arg
    // `param("dirname", p)` and `${p##*/}` as `param("basename", p)`
    // (pattern implied by the op). Both are the SAME cut identities as
    // below — and for `dirname` that is load-bearing: the shared
    // PathName render answers GNU dirname COMMAND semantics (no slash →
    // ".", leading slash → "/"), which diverge from bash's STRING strip
    // (`${p%/*}` leaves a slashless value unchanged and yields "" for
    // "/lead") — measured on the smoke corpus. The CutAt arm renders
    // the string identity; targets without the arm keep the PathName
    // path (text_ops runs after this pass).
    if args.len() == 2 {
        let (IrExpr::Str(op, op_style), IrExpr::Str(name, _)) = (&args[0], &args[1]) else {
            return false;
        };
        let head = match op.as_str() {
            "dirname" => true,   // `${p%/*}`   — head before LAST `/`
            "basename" => false, // `${p##*/}`  — tail after LAST `/`
            _ => return false,
        };
        if !is_text_operand_name(name) || !cut_target_ok() {
            return false;
        }
        let text = IrExpr::Call {
            func: "param".to_string(),
            args: vec![
                IrExpr::Str(String::new(), op_style.clone()),
                IrExpr::Str(name.clone(), StrStyle::DoubleQuoted),
            ],
        };
        *e = IrExpr::Ext(Box::new(CutAt {
            text,
            delim: "/".to_string(),
            head,
            first: false,
        }));
        return true;
    }
    // The strip form is exactly 3 args: (op, name, pattern). A 4th arg is
    // the `/`-substitute shape (`param("/", name, pat, repl)`) — refuse.
    if args.len() != 3 {
        return false;
    }
    let (IrExpr::Str(op, op_style), IrExpr::Str(name, _), IrExpr::Str(pat, pat_style)) =
        (&args[0], &args[1], &args[2])
    else {
        return false;
    };
    if !matches!(op.as_str(), "#" | "##" | "%" | "%%") {
        return false;
    }
    if !is_text_operand_name(name) {
        return false;
    }

    // Classify the pattern → replacement (REFUSE > GUESS at every step).
    // `repl_pat` is the payload the replacement carries: the pattern as
    // written for the literal branch, the star's BODY for star forms
    // (`${v%/*}` cuts on `/`, never on the `/*` spelling).
    let (repl, repl_pat) = if is_literal_pattern(pat) {
        (
            Replacement::Affix {
                prefix: matches!(op.as_str(), "#" | "##"),
            },
            pat.clone(),
        )
    } else if let Some((body, star_first)) = single_star_shape(pat) {
        // Exactly one `*`, pinned at an end. The body must be magic-free:
        // `?`/`[`/`\` would make the glob match diverge from a plain
        // find/rfind cut (a second/middle star is refused by the shape).
        if body.contains(['?', '[', '\\']) {
            return false;
        }
        let r = match (op.as_str(), star_first) {
            // `#lit*` ≡ `#lit` (the shortest prefix matching lit·X is
            // lit itself); `%*lit` ≡ `%lit` — the EXISTING AffixStrip.
            ("#", false) => Replacement::Affix { prefix: true },
            // `%*` (EMPTY body — the bare `*`) is the identity, checked
            // BEFORE the general `%*lit` ≡ `%lit` arm below: the shortest
            // suffix matching `*` is the EMPTY suffix (a star matches
            // empty; verified vs bash: `v=abc` → `abc`), so the strip
            // removes nothing — the plain read. A semantic identity like
            // the `%%*` arm (target-independent, no gate), and the one
            // empty-body shape a target that REFUSES the Affix branch (c)
            // would otherwise keep as `param("%",v,"*")`: that helper's
            // loop stops at the FIRST NON-empty matching suffix ("c"
            // matches `*` → keeps "ab"), which diverges from bash.
            ("%", true) if body.is_empty() => {
                *e = IrExpr::Call {
                    func: "param".to_string(),
                    args: vec![
                        IrExpr::Str(String::new(), op_style.clone()),
                        IrExpr::Str(name.clone(), StrStyle::DoubleQuoted),
                    ],
                };
                return true;
            }
            ("%", true) => Replacement::Affix { prefix: false },
            // the four find()/rfind() cuts (module doc's identity table)
            ("%", false) => Replacement::Cut { head: true, first: false },
            ("%%", false) => Replacement::Cut { head: true, first: true },
            ("#", true) => Replacement::Cut { head: false, first: true },
            ("##", true) => Replacement::Cut { head: false, first: false },
            // `%%*` — the longest suffix matching `*` is the WHOLE value,
            // so the result is "" on every input (verified vs bash,
            // including the empty value) — expressible as the literal,
            // no conditional node needed.
            ("%%", true) if body.is_empty() => {
                *e = IrExpr::Str(String::new(), StrStyle::DoubleQuoted);
                return true;
            }
            // `##lit*` / `%%*lit` → ""-conditional on starts/ends-with;
            // no node expresses that shape yet — refuse.
            _ => return false,
        };
        (r, body.to_string())
    } else {
        return false;
    };

    // The target gates live HERE (not only in `transform`) so no entry
    // point — a direct `expr()`/`fold()` call, a nested pass — can bypass
    // them and fold for a backend without a verified render arm for the
    // node being produced.
    match repl {
        Replacement::Affix { .. } if !fold_target_ok() => return false,
        Replacement::Cut { .. } if !cut_target_ok() => return false,
        _ => {}
    }

    // Rebuild over the plain read. The plain read uses the SAME op-style
    // slot the parser filled, so the export round-trip stays stable.
    let text = IrExpr::Call {
        func: "param".to_string(),
        args: vec![
            IrExpr::Str(String::new(), op_style.clone()),
            IrExpr::Str(name.clone(), StrStyle::DoubleQuoted),
        ],
    };
    *e = match repl {
        Replacement::Affix { prefix } => IrExpr::Ext(Box::new(AffixStrip {
            text,
            pattern: IrExpr::Str(repl_pat, pat_style.clone()),
            prefix,
        })),
        Replacement::Cut { head, first } => IrExpr::Ext(Box::new(CutAt {
            text,
            delim: repl_pat,
            head,
            first,
        })),
    };
    true
}

/// Exactly one `*`, pinned at either end → (body, star_is_first). The
/// body may be empty (the pattern `*`). A middle star (`a*b` — the cut
/// point would need a scan), a second star, or no star → None.
fn single_star_shape(p: &str) -> Option<(&str, bool)> {
    if p.matches('*').count() != 1 {
        return None;
    }
    if p.starts_with('*') {
        Some((&p[1..], true))
    } else if p.ends_with('*') {
        Some((&p[..p.len() - 1], false))
    } else {
        None
    }
}

/// Targets whose CutAt render is VERIFIED end-to-end (rust renders
/// find/rfind + slicing; c renders the same single find/rfind cut into
/// a vbuf — neither drags the glob engine) AND whose current star-strip
/// render drags the glob helper. Other backends accept by adding a
/// render arm and their target here (the FieldGrab offer pattern).
/// Unset target (the shir embed path / non-CLI ingress) refuses — the
/// fold is a render-quality optimisation, never a semantic requirement.
fn cut_target_ok() -> bool {
    match crate::transforms::target() {
        Some(t) => matches!(t.as_str(), "rs" | "rust" | "c"),
        None => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ir::StrStyle;

    fn s(v: &str) -> IrExpr {
        IrExpr::Str(v.to_string(), StrStyle::DoubleQuoted)
    }

    fn strip(op: &str, name: &str, pat: &str) -> IrExpr {
        IrExpr::Call {
            func: "param".to_string(),
            args: vec![s(op), s(name), s(pat)],
        }
    }

    fn with_target<T>(t: Option<&str>, f: impl FnOnce() -> T) -> T {
        crate::transforms::set_target(t);
        let r = f();
        crate::transforms::set_target(None);
        r
    }

    fn is_affix(e: &IrExpr, prefix: bool, pat: &str) -> bool {
        match e {
            IrExpr::Ext(n) if n.tag() == "AffixStrip" => {
                let j = n.to_json();
                j["prefix"] == serde_json::Value::from(prefix)
                    && j["pattern"]["value"].as_str() == Some(pat)
            }
            _ => false,
        }
    }

    #[test]
    fn literal_prefix_strip_folds_on_rust() {
        with_target(Some("rs"), || {
            let mut e = strip("#", "v", "git-");
            assert!(expr(&mut e));
            assert!(is_affix(&e, true, "git-"));
        });
    }

    #[test]
    fn literal_suffix_strip_folds_on_java() {
        with_target(Some("java"), || {
            let mut e = strip("%", "v", "-branch");
            assert!(expr(&mut e));
            assert!(is_affix(&e, false, "-branch"));
        });
    }

    #[test]
    fn go_is_refused_until_its_ext_path_is_wired() {
        // Go's Ext fallback emits a sh2.AffixStrip stub (not a refusal),
        // so the fold must NOT fire there yet.
        with_target(Some("go"), || {
            let mut e = strip("#", "v", "lit");
            let before = format!("{e:?}");
            assert!(!expr(&mut e), "go must refuse until its Ext arm is wired");
            assert_eq!(format!("{e:?}"), before);
        });
    }

    #[test]
    fn greedy_and_shortest_are_identical_for_a_literal() {
        // `##` (longest) on a magic-free literal == `#` (shortest): the
        // fold maps both to the same single-occurrence AffixStrip.
        with_target(Some("java"), || {
            let mut e = strip("##", "v", "git-");
            assert!(expr(&mut e));
            assert!(is_affix(&e, true, "git-"));
            let mut e = strip("%%", "v", "-branch");
            assert!(expr(&mut e));
            assert!(is_affix(&e, false, "-branch"));
        });
    }

    #[test]
    fn glob_magic_pattern_is_kept() {
        // Patterns whose cut point needs a real glob engine — never fold:
        // a middle star (scan for the match), `?`, a class, or a magic
        // char inside the star-form body.
        with_target(Some("rs"), || {
            for pat in ["a*b", "a?b", "[a-z]*", "a*b*", "x[y]*", "a\\*b"] {
                let mut e = strip("#", "v", pat);
                let before = format!("{e:?}");
                assert!(!expr(&mut e), "must keep glob pattern {pat:?}");
                assert_eq!(format!("{e:?}"), before);
            }
        });
    }

    fn is_cut(e: &IrExpr, head: bool, first: bool, delim: &str) -> bool {
        match e {
            IrExpr::Ext(n) if n.tag() == "CutAt" => {
                let j = n.to_json();
                j["head"] == serde_json::Value::from(head)
                    && j["first"] == serde_json::Value::from(first)
                    && j["delim"].as_str() == Some(delim)
            }
            _ => false,
        }
    }

    #[test]
    fn path_strip_param_shapes_fold_to_cuts() {
        // The parser's 2-arg path spellings: `${p%/*}` →
        // param("dirname", p), `${p##*/}` → param("basename", p).
        // The dirname arm is the STRING identity (slashless value
        // unchanged), which diverges from the dirname COMMAND render.
        with_target(Some("rs"), || {
            let mut e = IrExpr::Call {
                func: "param".to_string(),
                args: vec![s("dirname"), s("p")],
            };
            assert!(expr(&mut e));
            assert!(is_cut(&e, true, false, "/"));

            let mut e = IrExpr::Call {
                func: "param".to_string(),
                args: vec![s("basename"), s("p")],
            };
            assert!(expr(&mut e));
            assert!(is_cut(&e, false, false, "/"));

            // off rust: unchanged (text_ops lowers them to PathName as
            // before — that lane's behavior is untouched)
            let mut e = IrExpr::Call {
                func: "param".to_string(),
                args: vec![s("dirname"), s("p")],
            };
            let before = format!("{e:?}");
            with_target(Some("pl"), || assert!(!expr(&mut e)));
            assert_eq!(format!("{e:?}"), before);
        });
    }

    #[test]
    fn star_forms_fold_to_the_four_cuts_on_rust() {
        // The bash-verified identity table: every (op, star-position)
        // maps to its head/tail × first/last cut.
        with_target(Some("rs"), || {
            // %lit*  — head before LAST (dirname idiom: ${path%/*})
            let mut e = strip("%", "path", "/*");
            assert!(expr(&mut e));
            assert!(is_cut(&e, true, false, "/"));
            // %%lit* — head before FIRST
            let mut e = strip("%%", "path", "/*");
            assert!(expr(&mut e));
            assert!(is_cut(&e, true, true, "/"));
            // #*lit  — tail after FIRST
            let mut e = strip("#", "path", "*/");
            assert!(expr(&mut e));
            assert!(is_cut(&e, false, true, "/"));
            // ##*lit — tail after LAST (basename idiom: ${path##*/})
            let mut e = strip("##", "path", "*/");
            assert!(expr(&mut e));
            assert!(is_cut(&e, false, false, "/"));
            // a literal body, star at the end
            let mut e = strip("%", "v", "o*");
            assert!(expr(&mut e));
            assert!(is_cut(&e, true, false, "o"));
        });
    }

    #[test]
    fn star_alone_is_the_identity_family_and_folds() {
        // Pattern `*`: body empty — each op keeps its verified meaning
        // (`#*` identity, `##*` → "", `%*` identity, `%%*` → "" is the
        // refused ""-conditional class only for `%%`+start… which maps
        // through the Cut arms below, all four verified vs bash).
        with_target(Some("rs"), || {
            let mut e = strip("#", "v", "*");
            assert!(expr(&mut e));
            assert!(is_cut(&e, false, true, ""));
            let mut e = strip("##", "v", "*");
            assert!(expr(&mut e));
            assert!(is_cut(&e, false, false, ""));
            // `%` + start-star: with the BARE `*` the shortest matching
            // suffix is the empty one, so nothing is stripped — the
            // identity is the PLAIN READ (the `%*lit` ≡ `%lit` Affix arm
            // still applies to a non-empty body, asserted below)
            let mut e = strip("%", "v", "*");
            assert!(expr(&mut e));
            assert!(
                matches!(&e, IrExpr::Call { func, args }
                    if func == "param"
                        && matches!(&args[..], [IrExpr::Str(op, _), IrExpr::Str(n, _)]
                            if op.is_empty() && n == "v")),
                "%* → the plain read, got {e:?}"
            );
            // `%%`+`*` folds to the empty literal (the longest suffix
            // matching `*` is the whole value — always "")
            let mut e = strip("%%", "v", "*");
            assert!(expr(&mut e));
            assert!(matches!(&e, IrExpr::Str(s, _) if s.is_empty()), "%%* → empty Str, got {e:?}");
        });
    }

    #[test]
    fn star_forms_map_to_affixstrip_where_the_identity_is_a_strip() {
        with_target(Some("rs"), || {
            // #lit* ≡ #lit: shortest prefix matching lit·X is lit itself
            let mut e = strip("#", "v", "git*");
            assert!(expr(&mut e));
            assert!(is_affix(&e, true, "git"));
            // %*lit ≡ %lit
            let mut e = strip("%", "v", "*tch");
            assert!(expr(&mut e));
            assert!(is_affix(&e, false, "tch"));
        });
    }

    #[test]
    fn conditional_star_forms_are_refused() {
        // `##lit*` / `%%*lit` → "" iff starts-/ends-with: no node
        // expresses the conditional yet — refuse on every target.
        with_target(Some("rs"), || {
            for (op, pat) in [("##", "git*"), ("%%", "*tch")] {
                let mut e = strip(op, "v", pat);
                let before = format!("{e:?}");
                assert!(!expr(&mut e), "must refuse {op}{pat:?}");
                assert_eq!(format!("{e:?}"), before);
            }
        });
    }

    #[test]
    fn cut_forms_are_accepted_only_where_an_arm_exists() {
        // CutAt is offered per backend as render arms land (rust first,
        // then c); the AffixStrip shapes keep their own java acceptance.
        for t in ["rs", "rust", "c"] {
            with_target(Some(t), || {
                let mut e = strip("%", "path", "/*");
                assert!(expr(&mut e), "target {t} renders CutAt, must fold");
                assert!(is_cut(&e, true, false, "/"));
            });
        }
        with_target(Some("java"), || {
            let mut e = strip("%", "path", "/*");
            let before = format!("{e:?}");
            assert!(!expr(&mut e), "java must refuse the CutAt offer");
            assert_eq!(format!("{e:?}"), before);

            let mut e = strip("#", "v", "git*");
            assert!(expr(&mut e), "java still folds the Affix star shape");
            assert!(is_affix(&e, true, "git"));
        });
        // c has a CutAt arm but NO AffixStrip arm: the plain-literal and
        // Affix-star shapes must still refuse there (no glob-free render
        // to fold to), so the cold literal sites keep today's path.
        with_target(Some("c"), || {
            for (op, pat) in [("#", "lit"), ("#", "git*"), ("%", "*tch")] {
                let mut e = strip(op, "v", pat);
                let before = format!("{e:?}");
                assert!(!expr(&mut e), "c must refuse the Affix shape {op}{pat:?}");
                assert_eq!(format!("{e:?}"), before);
            }
        });
        for t in ["pl", "go", "js"] {
            with_target(Some(t), || {
                let mut e = strip("%", "path", "/*");
                let before = format!("{e:?}");
                assert!(!expr(&mut e), "target {t} must refuse CutAt");
                assert_eq!(format!("{e:?}"), before);
            });
        }
    }

    #[test]
    fn non_identifier_name_is_kept() {
        // `arr[i]` is NOT in this list: a bare-subscript element read is
        // a scalar string operand and folds (see the test below).
        with_target(Some("rs"), || {
            for name in ["arr[@]", "arr[*]", "arr[]", "arr[x+1]", "@", "*", "#arr", "!pre", "1", "x.y", ""] {
                let mut e = strip("#", name, "lit");
                assert!(!expr(&mut e), "must keep non-identifier name {name:?}");
            }
        });
    }

    #[test]
    fn array_element_read_folds_as_text_operand() {
        // `${args[i]#--}`: the element read is one string value, rust
        // renders param("", "args[i]") via the same `array_index_name`
        // the strip arm used, java documents that arrival shape — fold
        // like any scalar read.
        with_target(Some("rs"), || {
            for name in ["args[i]", "args[0]", "args[idx]"] {
                let mut e = strip("#", name, "--");
                assert!(expr(&mut e), "element read {name:?} must fold");
                assert!(is_affix(&e, true, "--"));
            }
        });
    }

    #[test]
    fn substitute_op_is_kept() {
        // `/` and `//` carry a replacement arg and mean substitute, not
        // strip — never fold (and their arity differs anyway).
        with_target(Some("rs"), || {
            for op in ["/", "//"] {
                let mut e = IrExpr::Call {
                    func: "param".to_string(),
                    args: vec![s(op), s("v"), s("lit")],
                };
                assert!(!expr(&mut e), "must keep substitute op {op:?}");
            }
        });
    }

    #[test]
    fn non_affix_target_is_refused() {
        // estree/perl/python inline literals optimally; c/sh/zig have no
        // AffixStrip arm. All refuse — nothing changes.
        for t in ["js", "estree", "pl", "perl", "py", "python", "c", "sh", "zig", "go"] {
            with_target(Some(t), || {
                let mut e = strip("#", "v", "lit");
                let before = format!("{e:?}");
                assert!(!expr(&mut e), "target {t} must refuse the fold");
                assert_eq!(format!("{e:?}"), before);
            });
        }
    }

    #[test]
    fn unset_target_is_refused() {
        with_target(None, || {
            let mut e = strip("#", "v", "lit");
            assert!(!expr(&mut e));
        });
    }

    #[test]
    fn traversal_reaches_nested_positions() {
        with_target(Some("rs"), || {
            let inner = strip("#", "v", "lit");
            let mut st = IrStmt::Expr(IrExpr::Call {
                func: "builtin".to_string(),
                args: vec![s("echo"), IrExpr::Array(vec![inner])],
            });
            assert!(stmt(&mut st));
            let IrStmt::Expr(IrExpr::Call { args, .. }) = &st else {
                panic!("stmt shape changed")
            };
            let IrExpr::Array(items) = &args[1] else {
                panic!("expect array")
            };
            assert!(is_affix(&items[0], true, "lit"), "folded through nesting");
        });
    }

    #[test]
    fn transform_reports_false_when_nothing_matches() {
        with_target(Some("rs"), || {
            // a middle star is never foldable — nothing matches
            let mut stmts = vec![IrStmt::Expr(strip("#", "v", "a*b"))];
            assert!(!transform(&mut stmts));
        });
    }

    #[test]
    fn transform_reports_true_and_rewrites() {
        with_target(Some("rs"), || {
            let mut stmts = vec![IrStmt::Expr(strip("#", "v", "lit"))];
            assert!(transform(&mut stmts));
            let IrStmt::Expr(e) = &stmts[0] else {
                panic!()
            };
            assert!(is_affix(e, true, "lit"));
        });
    }

    /// End-to-end through the REAL pipeline: the parser lowers `${v#git-}`
    /// to `param("#", "v", "git-")` and, with the target set to rust,
    /// `ast_to_ir` (which runs the registry) folds it to AffixStrip. Proves
    /// the registry wiring, not just `fold()`.
    #[test]
    fn parser_to_registry_folds_literal_strip() {
        with_target(Some("rs"), || {
            let cmds = crate::Parser::new("v=git-x\necho \"${v#git-}\"\n")
                .parse()
                .expect("parse");
            let prog = crate::shir::ast_to_ir(&cmds);
            fn has_affix(e: &IrExpr) -> bool {
                match e {
                    IrExpr::Ext(n) if n.tag() == "AffixStrip" => true,
                    IrExpr::Call { args, .. } => args.iter().any(has_affix),
                    IrExpr::Array(items) => items.iter().any(has_affix),
                    IrExpr::Interpolate(parts) => parts
                        .iter()
                        .any(|p| matches!(p, crate::ir::InterpPart::Expr(x) if has_affix(x))),
                    _ => false,
                }
            }
            fn walk(sts: &[IrStmt]) -> bool {
                sts.iter().any(|s| match s {
                    IrStmt::Expr(e) | IrStmt::Output { value: e, .. } | IrStmt::Assign { expr: e, .. } => has_affix(e),
                    IrStmt::If { then, else_, .. } => walk(then) || walk(else_),
                    _ => false,
                })
            }
            // The raw param("#"/"##"/"%"/"%%", …) call must never survive
            // the registry on a strip-capable target — that is the gate
            // this pass owns (its render would drag the glob engine).
            fn has_raw_param_strip(e: &IrExpr) -> bool {
                match e {
                    IrExpr::Call { func, args } => {
                        (func == "param"
                            && matches!(args.first(),
                                Some(IrExpr::Str(op, _))
                                    if matches!(op.as_str(), "#" | "##" | "%" | "%%")))
                            || args.iter().any(has_raw_param_strip)
                    }
                    IrExpr::Array(items) => items.iter().any(has_raw_param_strip),
                    IrExpr::Interpolate(parts) => parts.iter().any(
                        |p| matches!(p, crate::ir::InterpPart::Expr(x) if has_raw_param_strip(x))),
                    _ => false,
                }
            }
            fn walk_raw(sts: &[IrStmt]) -> bool {
                sts.iter().any(|s| match s {
                    IrStmt::Expr(e) | IrStmt::Output { value: e, .. } | IrStmt::Assign { expr: e, .. } => has_raw_param_strip(e),
                    IrStmt::If { then, else_, .. } => walk_raw(then) || walk_raw(else_),
                    _ => false,
                })
            }
            // Two acceptable end states: the AffixStrip node stands
            // (non-const text — nothing better exists), or — v is a
            // single-def const here — copy-propagation (registered after
            // this pass) folded the WHOLE strip to the literal result,
            // which is strictly better: no node, no helper, no glob
            // engine, for every backend.
            assert!(
                walk(&prog.stmts) || !walk_raw(&prog.stmts),
                "literal strip must fold to AffixStrip or a literal through the registry"
            );
        });
    }
}
