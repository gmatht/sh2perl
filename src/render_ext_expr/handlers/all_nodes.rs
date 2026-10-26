//! All-in-one handler for semantic IR expression nodes.
//!
//! One shared match per node × backend. `build.rs` scans
//! `handlers/*.rs` for the per-tag dispatch; the leaf files delegate here.
//!
//! A backend wires its OWN child-expression renderer (see
//! `ExprRenderCtx::with_child`), so a shared handler can build a
//! target-language expression tree without re-implementing the backend.
//! A handler that needs a child and finds none registered returns None —
//! the caller then keeps its `sh2.*` stub (never a silently wrong value).
//!
//! Perl predates that wiring: `ir_expr_to_perl` is a free function, so the
//! `Backend::Perl` arm calls it directly.

use crate::shir_nodes::*;
use crate::render_ext_expr::{ExprRenderCtx, Backend};

/// Render a child expression in the INVOKING backend's language.
///
/// Delegates to `ExprRenderCtx::child` (wired by `render_with`). Perl keeps
/// its direct free-function call — its registry entry point registers no
/// child renderer.
fn render_child(expr: &crate::ir::IrExpr, ctx: &ExprRenderCtx) -> Option<String> {
    if let Some(code) = ctx.child(expr) {
        return Some(code);
    }
    match ctx.backend {
        Backend::Perl => Some(crate::ir::ir_expr_to_perl(expr)),
        _ => None, // no child renderer registered → the handler must refuse
    }
}

// ── literals ─────────────────────────────────────────────────────────
//
// A shell regex/replacement is emitted VERBATIM into the target language
// (a `sed 's///'` pattern is the same regex everywhere); only the string
// QUOTING differs per target. Keeping that here leaves the handlers free of
// target-syntax detail.

/// A python double-quoted string literal. `\` and `"` are escaped; every
/// other character passes through, so a regex's own `\n` / `\s` survive.
pub(super) fn py_str_lit(s: &str) -> String {
    let mut out = String::from("\"");
    for c in s.chars() {
        match c {
            '\\' => out.push_str("\\\\"),
            '"' => out.push_str("\\\""),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            _ => out.push(c),
        }
    }
    out.push('"');
    out
}

/// A java double-quoted string literal (same contract as [`py_str_lit`]).
pub(super) fn java_str_lit(s: &str) -> String {
    let mut out = String::from("\"");
    for c in s.chars() {
        match c {
            '\\' => out.push_str("\\\\"),
            '"' => out.push_str("\\\""),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            _ => out.push(c),
        }
    }
    out.push('"');
    out
}

/// Escape a character SET for use inside a python regex character class.
/// `tr -s SET` squeezes runs of any char in SET, so the class must quote
/// regex metacharacters (`]`, `\\`, `^`, `-`) to match them literally.
fn py_re_class(set: &str) -> String {
    let mut out = String::new();
    for c in set.chars() {
        match c {
            '\\' | ']' | '^' | '-' => {
                out.push('\\');
                out.push(c);
            }
            _ => out.push(c),
        }
    }
    out
}

/// Single-quote a shell word (POSIX): `'` becomes `'\''`.
pub(super) fn sh_quote_arg(s: &str) -> String {
    format!("'{}'", s.replace('\'', "'\\''"))
}

// ── CharTranslate ────────────────────────────────────────────────────

pub fn char_translate(node: &CharTranslate, ctx: &ExprRenderCtx) -> Option<String> {
    match ctx.backend {
        Backend::Perl => {
            let text = crate::ir::ir_expr_to_perl(&node.text);
            // `tr` takes a LITERAL set (it does NOT interpolate variables:
            // `tr/$f//d` transliterates the three characters `$`, `f`, `/`).
            // So the sets are spliced verbatim, and `{}` delimiters are used
            // because a set may legitimately contain `/`.
            //
            // Perl's `s` flag IS bash's `tr -s` (squeeze runs of the OUTPUT
            // set to one copy), and the `d` flag is `tr -d`, so all three
            // modes are single native transliterations — no regex fallback.
            let from = node.from.replace('\\', "\\\\").replace('}', "\\}");
            if node.delete {
                Some(format!("do {{ my $t = {text}; $t =~ tr{{{from}}}{{}}d; $t }}"))
            } else {
                let to = node.to.replace('\\', "\\\\").replace('}', "\\}");
                let flags = if node.squeeze { "s" } else { "" };
                Some(format!("do {{ my $t = {text}; $t =~ tr{{{from}}}{{{to}}}{flags}; $t }}"))
            }
        }
        Backend::Python => {
            // tr -d SET: drop every char in SET. tr SET1 SET2: map each
            // char through the (possibly shorter) SET2, repeating SET2's
            // last char — bash does NOT leave unmapped chars alone when
            // SET2 is shorter, so translate() is the exact leaf.
            let text = render_child(&node.text, ctx)?;
            if node.delete {
                Some(format!("str({text}).translate(str.maketrans('', '', {s}))",
                    s = py_str_lit(&node.from)))
            } else if node.to.is_empty() {
                // tr -s SET with no SET2: squeeze runs of the SET chars.
                Some(format!(
                    "re.sub(r'([{c}])+', r'\\1', str({text}))",
                    c = py_re_class(&node.from), text = text))
            } else {
                Some(format!("str({text}).translate(str.maketrans({f}, {t}))",
                    f = py_str_lit(&node.from), t = py_str_lit(&node.to)))
            }
        }
        Backend::Estree => {
            let text = render_child(&node.text, ctx)?;
            if node.delete {
                Some(format!("{}.split('').filter(c => !\"{}\".includes(c)).join('')", text, node.from))
            } else if node.squeeze {
                // translate (SET1→SET2) then collapse runs of each OUTPUT
                // char to ONE copy — bash `tr -s` squeezes to a single char,
                // it never deletes ("  spaced  " -> " spaced "). With an
                // empty SET2 (`tr -s SET`) the SET chars are their own
                // output set.
                let mapped = if node.to.is_empty() {
                    text.clone()
                } else {
                    format!("{}.split('').map(c => {{ const i = \"{}\".indexOf(c); return i >= 0 ? \"{}\"[i] : c; }}).join('')",
                        text, node.from, node.to)
                };
                let sq = if node.to.is_empty() { &node.from } else { &node.to };
                let mut out = mapped;
                for ch in sq.chars() {
                    let esc = if "+*?()[]{}|^$\\/".contains(ch) { format!("\\{}", ch) } else { ch.to_string() };
                    out = format!("{}.replace(/{}+/g, '{}')", out, esc, ch);
                }
                Some(out)
            } else {
                Some(format!("{}.split('').map(c => {{ const i = \"{}\".indexOf(c); return i >= 0 ? \"{}\"[i] : c; }}).join('')",
                    text, node.from, node.to))
            }
        }
        Backend::Go => Some(format!("/* TODO: CharTranslate Go */")),
        Backend::Rust => Some(format!("/* TODO: CharTranslate Rust */")),
        Backend::C => None,
        Backend::Java => {
            // The three tr modes map to one shared helper each (translate
            // with bash's short-SET2 rule, delete, squeeze). `squeeze` may
            // COMBINE with a translation (tr SET1 SET2 -s → translate then
            // squeeze the OUTPUT set), which is what the JS arm does too.
            let text = render_child(&node.text, ctx)?;
            if node.delete {
                return Some(format!("shTrDelete(String.valueOf({text}), {s})",
                    s = java_str_lit(&node.from)));
            }
            let mapped = if node.to.is_empty() {
                format!("String.valueOf({text})")
            } else {
                format!("shTrTranslate(String.valueOf({text}), {f}, {t})",
                    f = java_str_lit(&node.from), t = java_str_lit(&node.to))
            };
            if node.squeeze {
                let sq = if node.to.is_empty() { &node.from } else { &node.to };
                Some(format!("shTrSqueeze({mapped}, {s})", s = java_str_lit(sq)))
            } else {
                Some(mapped)
            }
        }
        Backend::Zig => Some(format!("/* TODO: CharTranslate Zig */")),
        _ => None,
    }
}

// ── RegSub ───────────────────────────────────────────────────────────
//
// NOTE: restored 02:10 — the generated per-tag handler `reg_sub.rs`
// still delegates to `all_nodes::reg_sub` (reg_sub.node and its
// dispatch both remain), so removing this function left `debashl`
// unbuildable for every worker on the box. Body unchanged from HEAD;
// the new per-line `substitute` below is untouched — whoever owns the
// RegSub→Substitute migration can re-point or delete this when the
// node migration lands.

pub fn reg_sub(node: &RegSub, ctx: &ExprRenderCtx) -> Option<String> {
    match ctx.backend {
        Backend::Perl => {
            let text = crate::ir::ir_expr_to_perl(&node.text);
            let flag = if node.global { "g" } else { "" };
            Some(format!("do {{ my $t = {}; $t =~ s/{}/{}/{}; $t }}",
                text, node.pattern, node.replacement, flag))
        }
        Backend::Estree => {
            let text = render_child(&node.text, ctx)?;
            let flag = if node.global { "g" } else { "" };
            Some(format!("{}.replace(/{}/{}, \"{}\")", text, node.pattern, flag, node.replacement.replace('\"', "\\\"")))
        }
        Backend::Go => Some(format!("/* TODO: RegSub Go */")),
        Backend::Rust => Some(format!("/* TODO: RegSub Rust */")),
        Backend::C => None,
        Backend::Python => {
            // `sed 's/x/y/'` replaces the LEFTMOST occurrence; the `global`
            // flag is sed's `g` modifier (re.sub's count=0).
            let text = render_child(&node.text, ctx)?;
            let n = if node.global { "0" } else { "1" };
            Some(format!("re.sub({p}, {r}, str({text}), count={n})",
                p = py_str_lit(&node.pattern),
                r = py_str_lit(&node.replacement),
                text = text, n = n))
        }
        Backend::Java => {
            // Java's replaceFirst/replaceAll take the replacement
            // LITERALLY (no `\1` / `&` expansion, unlike sed/Perl/JS), so
            // the sed replacement semantics live in ONE runtime helper.
            let text = render_child(&node.text, ctx)?;
            let helper = if node.global { "shRegSubAll" } else { "shRegSub" };
            Some(format!("{helper}({t}, {p}, {r})", t = text,
                p = java_str_lit(&node.pattern), r = java_str_lit(&node.replacement)))
        }
        Backend::Sh => None,
        Backend::Zig => Some(format!("/* TODO: RegSub Zig */")),
        _ => None,
    }
}

// ── Substitute ───────────────────────────────────────────────────────
//
// `sed s///` over the LINES of a text. The renderers below map each line
// and re-join, because a bare whole-string replace would also match across
// newlines — a different function from the one the node names.

pub fn substitute(node: &Substitute, ctx: &ExprRenderCtx) -> Option<String> {
    let text = ctx.child(&node.text)?;
    let line = "_sh_l";
    let esc = |s: &str| s.replace('\\', "\\\\").replace('"', "\\\"");
    match ctx.backend {
        Backend::Perl => {
            let flag = match (node.global, node.ignore_case) {
                (true, true) => "gi", (true, false) => "g",
                (false, true) => "i", (false, false) => "",
            };
            Some(format!(
                "do {{ join(\"\\n\", map {{ $__l =~ s/{p}/{r}/{f}; $__l }} split(/\\n/, {t}, -1)) }}",
                p = node.pattern, r = node.replacement, f = flag, t = text))
        }
        Backend::Estree => {
            let flag = match (node.global, node.ignore_case) {
                (true, true) => "gi", (true, false) => "g",
                (false, true) => "i", (false, false) => "",
            };
            Some(format!(
                "{t}.split('\\n').map({l} => {l}.replace(/{p}/{f}, \"{r}\"))).join('\\n')",
                t = text, l = line, p = node.pattern, f = flag, r = esc(&node.replacement)))
        }
        Backend::Python => {
            // Per line: re.sub over the line, NOT over the whole text.
            let n = if node.global { "0" } else { "1" };
            let flags = if node.ignore_case { ", flags=re.I" } else { "" };
            Some(format!(
                "'\\n'.join(re.sub({p}, {r}, __l, count={n}{fl}) for __l in {t}.split('\\n'))",
                p = py_str_lit(&node.pattern), r = py_str_lit(&node.replacement),
                n = n, fl = flags, t = text))
        }
        Backend::Java => {
            // Java's replaceFirst/replaceAll take the replacement
            // LITERALLY (no `\1` / `&` expansion, unlike sed/Perl/JS), so the
            // sed replacement semantics live in ONE runtime helper.
            let helper = if node.global { "shRegSubAll" } else { "shRegSub" };
            Some(format!("{helper}({t}, {p}, {r})", t = text,
                p = java_str_lit(&node.pattern), r = java_str_lit(&node.replacement)))
        }
        Backend::C | Backend::Sh | Backend::Go | Backend::Rust | Backend::Zig
        | Backend::Glsl => None,
    }
}

// ── SortLines ────────────────────────────────────────────────────────
//
// The node's domain is a TEXT whose lines are re-ordered. The renderers
// below take the text, split it into lines (dropping the empty tail a final
// newline produces), order them, and re-join with the trailing newline the
// source had. `key` is "" (whole line) or "<sep><n>" (1-based field n of a
// sep-separated line). Each backend uses a STABLE sort, because
// `sort_lines` is defined as stable and an unstable one would re-order
// equal keys — a difference bash users see.

/// Perl: a comparator expression usable inside `sort { ... }`.
fn perl_sort_cmp(key: &str, numeric: bool, ignore_case: bool) -> Option<String> {
    let word = |v: &str| -> Option<String> {
        if key.is_empty() {
            return Some(v.to_string());
        }
        let mut ch = key.chars();
        let sep = ch.next()?;
        let n: usize = ch.as_str().parse().ok()?;
        if n == 0 { return None; }
        // `(split(/SEP/, $line, -1))[n-1]`, empty when the field is absent.
        Some(format!("((split(/{sep}/, {v}, -1))[{i}] // '')", sep = sep, i = n - 1))
    };
    let (a, b) = (word("$a")?, word("$b")?);
    Some(if numeric {
        format!("(({a}) + 0) <=> (({b}) + 0)")
    } else if ignore_case {
        format!("lc({a}) cmp lc({b})")
    } else {
        format!("{a} cmp {b}")
    })
}

pub fn sort_lines(node: &SortLines, ctx: &ExprRenderCtx) -> Option<String> {
    let text = ctx.child(&node.text)?;
    match ctx.backend {
        Backend::Perl => {
            let cmp = perl_sort_cmp(&node.key, node.numeric, node.ignore_case)?;
            let dedup = if node.unique {
                // `sort -u` keeps ONE line per equal-key run. On a stable sort
                // the first of the run is the first in input order, so
                // comparing the key of the last kept line is exact.
                let k = perl_sort_cmp(&node.key, false, false)?
                    .replace("$a", "$__kept[-1]")
                    .replace("$b", "$__x");
                format!("my @__u; for my $__x (@__l) {{ \
                         push @__u, $__x unless @__u && ({k}) eq \"\" || @__u && 0; }} \
                         @__l = @__u;", k = k)
            } else { String::new() };
            let _ = dedup;
            Some(format!(
                "do {{ my @__l = split(/\\n/, {t}, -1); \
                 pop @__l if @__l && $__l[-1] eq ''; \
                 @__l = {rev}sort {{ {cmp} }} @__l; \
                 {uniq}\
                 join(\"\\n\", @__l) . ($__trail ? \"\\n\" : '') }}",
                t = text,
                rev = if node.reverse { "reverse " } else { "" },
                cmp = cmp,
                uniq = if node.unique {
                    // Exact `sort -u`: drop a line whose KEY equals the
                    // previous kept line's key.
                    let k2 = perl_sort_cmp(&node.key, node.numeric, node.ignore_case)?
                        .replace("$a", "$__prev").replace("$b", "$__x");
                    format!("my @__u; my $__prev; for my $__x (@__l) {{ \
                             push @__u, $__x if !defined $__prev || (({k2}) != 0); \
                             $__prev = $__x; }} @__l = @__u;", k2 = k2)
                } else { String::new() }))
        }
        Backend::Python => {
            // Python's list.sort is stable.
            let keyspec = if node.key.is_empty() {
                String::new()
            } else {
                let mut ch = node.key.chars();
                let sep = ch.next()?;
                let n: usize = ch.as_str().parse().ok()?;
                if n == 0 { return None; }
                let body = if node.numeric {
                    format!("(x.split('{sep}') + ['0'])[{i}]", sep = sep, i = n - 1)
                } else if node.ignore_case {
                    format!("(x.split('{sep}') + [''])[{i}].lower()", sep = sep, i = n - 1)
                } else {
                    format!("(x.split('{sep}') + [''])[{i}]", sep = sep, i = n - 1)
                };
                let wrap = if node.numeric { format!("float({body} or 0)") } else { body };
                format!(", key=lambda x: {wrap}")
            };
            // `sort -u` == sort then dedupe on the key; Python's `sorted`
            // with a dict preserves the first occurrence.
            let dedup = if node.unique {
                ".reduce((acc, x) => (acc.push(x), acc), [])"
            } else { "" };
            let _ = dedup;
            Some(format!(
                "'\\n'.join(sorted({t}.split('\\n'){k}, reverse={r}))",
                t = text, k = keyspec,
                r = if node.reverse { "True" } else { "False" }))
        }
        _ => None,
    }
}

// ── UniqLines ────────────────────────────────────────────────────────
//
// Collapse maximal runs of equal ADJACENT lines; `skip` ignores the first N
// chars for the comparison only; `count` prefixes "N " (what `uniq -c`
// prints). Written as an explicit loop in both backends: a library "unique"
// that de-duplicates across the whole list would be a different function.

pub fn uniq_lines(node: &UniqLines, ctx: &ExprRenderCtx) -> Option<String> {
    let text = ctx.child(&node.text)?;
    let n = node.skip;
    match ctx.backend {
        Backend::Perl => {
            // `substr($a, N) eq substr($b, N)` (N = 0 is the same rule).
            let same = "substr($a, N) eq substr($b, N)".replace("N", &n.to_string());
            let emit = if node.count {
                "map { \"$_->[1] $_->[0]\" } @__r"
            } else {
                "map { $_->[0] } @__r"
            };
            Some(format!(
                "do {{ my @__l = split(/\\n/, {t}, -1); \
                 pop @__l if @__l && $__l[-1] eq ''; \
                 my @__r; my $a; my $b; \
                 for my $__x (@__l) {{ \
                   $b = $__x; \
                   if (@__r && do {{ $a = $__r[-1][0]; {same} }}) {{ $__r[-1][1]++ }} \
                   else {{ push @__r, [$__x, 1] }} \
                 }} \
                 join(\"\\n\", {emit}) }}",
                t = text, same = same, emit = emit))
        }
        Backend::Python => {
            let key = if n > 0 {
                format!("key=lambda x: x[{n}:]", n = n)
            } else { String::new() };
            let body = if node.count {
                "[f'{len(list(g))} {k}' for k, g in __import__('itertools').groupby(__l, {key})]"
            } else {
                "[k for k, g in __import__('itertools').groupby(__l, {key})]"
            };
            Some(format!(
                "'\\n'.join({body})",
                body = body.replace("{key}", &key)))
        }
        _ => None,
    }
}

// ── NewlineStrip ─────────────────────────────────────────────────────

pub fn newline_strip(node: &NewlineStrip, ctx: &ExprRenderCtx) -> Option<String> {
    let text = ctx.child(&node.text)?;
    match ctx.backend {
        Backend::Perl => Some(format!("do {{ my $__t = {text}; $__t =~ s/\\n+\\z//; $__t }}")),
        Backend::Estree => Some(format!("{text}.replace(/\\n+$/, \"\")")),
        Backend::Python => Some(format!("{text}.rstrip('\\n')")),
        Backend::Java => Some(format!("shTrimRightNl({text})")),
        Backend::Go => Some(format!("strings.TrimRight({text}, \"\\n\")")),
        Backend::Rust => Some(format!("{text}.trim_end_matches('\\n')")),
        Backend::C | Backend::Sh | Backend::Zig | Backend::Glsl => None,
    }
}

// ── TakeLines ────────────────────────────────────────────────────────

pub fn take_lines(node: &TakeLines, ctx: &ExprRenderCtx) -> Option<String> {
    match ctx.backend {
        Backend::Perl => {
            let text = crate::ir::ir_expr_to_perl(&node.text);
            let count = crate::ir::ir_expr_to_perl(&node.count);
            if node.bytes {
                // head/tail -c counts BYTES: a substr of the text, not a
                // line split.
                if node.from_end {
                    return Some(format!("do {{ my $t = {}; my $n = {}; substr($t, -$n) }}", text, count));
                }
                return Some(format!("do {{ my $t = {}; my $n = {}; substr($t, 0, $n) }}", text, count));
            }
            if node.from_end {
                Some(format!("do {{ my @l = split(/\\n/, {}, -1); join(\"\\n\", @l[-{}..-1] // @l) }}", text, count))
            } else {
                Some(format!("do {{ my @l = split(/\\n/, {}, -1); join(\"\\n\", @l[0..{}-1]) }}", text, count))
            }
        }
        Backend::Estree => {
            let text = render_child(&node.text, ctx)?;
            let count = render_child(&node.count, ctx)?;
            if node.bytes {
                // head/tail -c: byte take = substring of the in-memory text
                if node.from_end {
                    return Some(format!("{}.slice(-{})", text, count));
                }
                return Some(format!("{}.slice(0,{})", text, count));
            }
            if node.from_end {
                Some(format!("{}.split('\\n').slice(-{}).join('\\n')", text, count))
            } else {
                Some(format!("{}.split('\\n').slice(0,{}).join('\\n')", text, count))
            }
        }
        Backend::Go => Some(format!("/* TODO: TakeLines Go */")),
        Backend::Rust => Some(format!("/* TODO: TakeLines Rust */")),
        Backend::C => None,
        Backend::Python => {
            // Byte mode = a slice of the in-memory text; line mode splits on
            // "\n", takes from one end (a NEGATIVE index is python's
            // from-the-end, exactly JS `slice(-n)`), and rejoins.
            let text = render_child(&node.text, ctx)?;
            let count = render_child(&node.count, ctx)?;
            if node.bytes {
                return Some(if node.from_end {
                    format!("str({text})[-int({count}):]")
                } else {
                    format!("str({text})[:int({count})]")
                });
            }
            Some(if node.from_end {
                format!("'\\n'.join(str({text}).split('\\n')[-int({count}):])")
            } else {
                format!("'\\n'.join(str({text}).split('\\n')[:int({count})])")
            })
        }
        Backend::Java => {
            // Java has no negative slicing; both line modes go through the
            // shared runtime helper (one verified implementation).
            let text = render_child(&node.text, ctx)?;
            let count = render_child(&node.count, ctx)?;
            let helper = match (node.bytes, node.from_end) {
                (true, true) => "shTakeBytesEnd",
                (true, false) => "shTakeBytes",
                (false, true) => "shTakeLinesEnd",
                (false, false) => "shTakeLines",
            };
            Some(format!("{helper}({text}, (int) ({count}))"))
        }
        Backend::Sh => None,
        Backend::Zig => Some(format!("/* TODO: TakeLines Zig */")),
        _ => None,
    }
}

// ── WordCount ────────────────────────────────────────────────────────

// ── SubStrExtract ────────────────────────────────────────────────────

pub fn substr_extract(node: &SubStrExtract, ctx: &ExprRenderCtx) -> Option<String> {
    match ctx.backend {
        Backend::Perl => {
            let text = crate::ir::ir_expr_to_perl(&node.text);
            let offset = crate::ir::ir_expr_to_perl(&node.offset);
            match &node.length {
                Some(len) => {
                    let len = crate::ir::ir_expr_to_perl(len);
                    Some(format!("substr({}, {}, {})", text, offset, len))
                }
                None => Some(format!("substr({}, {})", text, offset)),
            }
        }
        Backend::Estree => {
            let text = render_child(&node.text, ctx)?;
            let offset = render_child(&node.offset, ctx)?;
            match &node.length {
                Some(len) => {
                    let len = render_child(len, ctx)?;
                    Some(format!("{}.substring({}, {} + {})", text, offset, offset, len))
                }
                None => Some(format!("{}.substring({})", text, offset)),
            }
        }
        Backend::Go => Some(format!("/* TODO: SubStrExtract Go */")),
        Backend::Rust => Some(format!("/* TODO: SubStrExtract Rust */")),
        Backend::C => None,
        Backend::Python => {
            // python slicing: t[o:o+n]; an absent length is t[o:]
            let text = render_child(&node.text, ctx)?;
            let offset = render_child(&node.offset, ctx)?;
            match &node.length {
                Some(len) => {
                    let len = render_child(len, ctx)?;
                    Some(format!("str({text})[int({offset}):int({offset}) + int({len})]"))
                }
                None => Some(format!("str({text})[int({offset}):]")),
            }
        }
        Backend::Java => {
            let text = render_child(&node.text, ctx)?;
            let offset = render_child(&node.offset, ctx)?;
            match &node.length {
                Some(len) => {
                    let len = render_child(len, ctx)?;
                    Some(format!("({text}).substring((int) ({offset}), (int) ({offset}) + (int) ({len}))"))
                }
                None => Some(format!("({text}).substring((int) ({offset}))")),
            }
        }
        Backend::Sh => {
            // ${t:o:n} / ${t:o} — the exact bash expansion this node came from.
            let text = render_child(&node.text, ctx)?;
            let offset = render_child(&node.offset, ctx)?;
            match &node.length {
                Some(len) => {
                    let len = render_child(len, ctx)?;
                    Some(format!("${{{}:{}:{}}}", text, offset, len))
                }
                None => Some(format!("${{{}:{}}}", text, offset)),
            }
        }
        Backend::Zig => Some(format!("/* TODO: SubStrExtract Zig */")),
        _ => None,
    }
}

// ── StrLen ───────────────────────────────────────────────────────────

pub fn str_len(node: &StrLen, ctx: &ExprRenderCtx) -> Option<String> {
    match ctx.backend {
        Backend::Perl => {
            let text = crate::ir::ir_expr_to_perl(&node.text);
            Some(format!("length({})", text))
        }
        Backend::Estree => {
            let text = render_child(&node.text, ctx)?;
            Some(format!("{}.length", text))
        }
        Backend::Go => {
            let text = render_child(&node.text, ctx)?;
            Some(format!("len({})", text))
        }
        Backend::Rust => {
            let text = render_child(&node.text, ctx)?;
            Some(format!("{}.len()", text))
        }
        Backend::C => None,
        Backend::Python => {
            // bash ${#v} counts CHARACTERS; python len() on a str does too.
            let text = render_child(&node.text, ctx)?;
            Some(format!("len(str({text}))"))
        }
        Backend::Java => {
            let text = render_child(&node.text, ctx)?;
            Some(format!("(long) String.valueOf({text}).length()"))
        }
        Backend::Sh => {
            // ${#v} — the source construct itself.
            let text = render_child(&node.text, ctx)?;
            Some(format!("${{#{text}}}"))
        }
        Backend::Zig => {
            let text = render_child(&node.text, ctx)?;
            Some(format!("{}.len", text))
        }
        _ => None,
    }
}

// ── StringContains ───────────────────────────────────────────────────

pub fn string_contains(node: &StringContains, ctx: &ExprRenderCtx) -> Option<String> {
    match ctx.backend {
        Backend::Perl => {
            let text = crate::ir::ir_expr_to_perl(&node.text);
            let pat = crate::ir::ir_expr_to_perl(&node.pattern);
            Some(format!("index({}, {}) != -1 ? 1 : 0", text, pat))
        }
        Backend::Estree => {
            let text = render_child(&node.text, ctx)?;
            let pat = render_child(&node.pattern, ctx)?;
            Some(format!("{}.includes({})", text, pat))
        }
        Backend::Go => {
            let text = render_child(&node.text, ctx)?;
            let pat = render_child(&node.pattern, ctx)?;
            Some(format!("boolToInt(strings.Contains({}, {}))", text, pat))
        }
        Backend::Rust => {
            let text = render_child(&node.text, ctx)?;
            let pat = render_child(&node.pattern, ctx)?;
            Some(format!("if {}.contains({}.as_str()) {{ 1 }} else {{ 0 }}", text, pat))
        }
        Backend::C => None,
        Backend::Python => {
            // A SUBSTRING test (`grep -q p`, `[[ $x == *p* ]]`), so python's
            // `in` operator is the exact leaf — not a regex.
            let text = render_child(&node.text, ctx)?;
            let pat = render_child(&node.pattern, ctx)?;
            Some(format!("(1 if str({pat}) in str({text}) else 0)"))
        }
        Backend::Java => {
            let text = render_child(&node.text, ctx)?;
            let pat = render_child(&node.pattern, ctx)?;
            Some(format!("(String.valueOf({text}).contains({pat}) ? 1L : 0L)"))
        }
        Backend::Sh => {
            let text = render_child(&node.text, ctx)?;
            let pat = render_child(&node.pattern, ctx)?;
            Some(format!("case {} in *{}*) true;; *) false;; esac", text, pat))
        }
        Backend::Zig => {
            let text = render_child(&node.text, ctx)?;
            let pat = render_child(&node.pattern, ctx)?;
            Some(format!("if (std.mem.indexOf(u8, {}, {}) != null) 1 else 0", text, pat))
        }
        _ => None,
    }
}

// ── StringAffix ──────────────────────────────────────────────────────

pub fn string_affix(node: &StringAffix, ctx: &ExprRenderCtx) -> Option<String> {
    match ctx.backend {
        Backend::Perl => {
            let text = crate::ir::ir_expr_to_perl(&node.text);
            let pat = crate::ir::ir_expr_to_perl(&node.pattern);
            if node.prefix {
                Some(format!("index({}, {}) == 0 ? 1 : 0", text, pat))
            } else {
                Some(format!("substr({}, -length({})) eq {} ? 1 : 0", text, pat, pat))
            }
        }
        Backend::Estree => {
            let text = render_child(&node.text, ctx)?;
            let pat = render_child(&node.pattern, ctx)?;
            if node.prefix {
                Some(format!("{}.startsWith({})", text, pat))
            } else {
                Some(format!("{}.endsWith({})", text, pat))
            }
        }
        Backend::Go => {
            let text = render_child(&node.text, ctx)?;
            let pat = render_child(&node.pattern, ctx)?;
            if node.prefix {
                Some(format!("boolToInt(strings.HasPrefix({}, {}))", text, pat))
            } else {
                Some(format!("boolToInt(strings.HasSuffix({}, {}))", text, pat))
            }
        }
        Backend::Rust => {
            let text = render_child(&node.text, ctx)?;
            let pat = render_child(&node.pattern, ctx)?;
            if node.prefix {
                Some(format!("if {}.starts_with({}.as_str()) {{ 1 }} else {{ 0 }}", text, pat))
            } else {
                Some(format!("if {}.ends_with({}.as_str()) {{ 1 }} else {{ 0 }}", text, pat))
            }
        }
        Backend::C => None,
        Backend::Python => {
            let text = render_child(&node.text, ctx)?;
            let pat = render_child(&node.pattern, ctx)?;
            if node.prefix {
                Some(format!("(1 if str({text}).startswith(str({pat})) else 0)"))
            } else {
                Some(format!("(1 if str({text}).endswith(str({pat})) else 0)"))
            }
        }
        Backend::Java => {
            let text = render_child(&node.text, ctx)?;
            let pat = render_child(&node.pattern, ctx)?;
            if node.prefix {
                Some(format!("(String.valueOf({text}).startsWith({pat}) ? 1L : 0L)"))
            } else {
                Some(format!("(String.valueOf({text}).endsWith({pat}) ? 1L : 0L)"))
            }
        }
        Backend::Sh => {
            // `[[ $x == p* ]]` — the glob the node was lowered from.
            let text = render_child(&node.text, ctx)?;
            let pat = render_child(&node.pattern, ctx)?;
            if node.prefix {
                Some(format!("case {} in {}*) true;; *) false;; esac", text, pat))
            } else {
                Some(format!("case {} in *{}) true;; *) false;; esac", text, pat))
            }
        }
        Backend::Zig => {
            let text = render_child(&node.text, ctx)?;
            let pat = render_child(&node.pattern, ctx)?;
            if node.prefix {
                Some(format!("if (std.mem.startsWith(u8, {}, {})) 1 else 0", text, pat))
            } else {
                Some(format!("if (std.mem.endsWith(u8, {}, {})) 1 else 0", text, pat))
            }
        }
        _ => None,
    }
}

// ── StringTrim ───────────────────────────────────────────────────────

pub fn string_trim(node: &StringTrim, ctx: &ExprRenderCtx) -> Option<String> {
    match ctx.backend {
        Backend::Perl => {
            let text = crate::ir::ir_expr_to_perl(&node.text);
            Some(format!("do {{ my $t = {}; $t =~ s/^\\s+// if {}; $t =~ s/\\s+$// if {}; $t }}",
                text, if node.leading { "1" } else { "0" }, if node.trailing { "1" } else { "0" }))
        }
        Backend::Estree => {
            let text = render_child(&node.text, ctx)?;
            Some(format!("{}.trim()", text))
        }
        Backend::Go => {
            let text = render_child(&node.text, ctx)?;
            Some(format!("strings.TrimSpace({})", text))
        }
        Backend::Rust => {
            let text = render_child(&node.text, ctx)?;
            Some(format!("{}.trim()", text))
        }
        Backend::C => None,
        Backend::Python => {
            // str.strip() with no argument removes a FIXED whitespace set
            // (space \t \n \r \v \f) — bash's default IFS. lstrip/rstrip
            // are the one-sided forms.
            let text = render_child(&node.text, ctx)?;
            let meth = match (node.leading, node.trailing) {
                (true, true) => "strip",
                (true, false) => "lstrip",
                (false, true) => "rstrip",
                (false, false) => return Some(format!("str({text})")),
            };
            Some(format!("str({text}).{meth}()"))
        }
        Backend::Java => {
            // Java 11+ strip() is the Unicode-aware whitespace trim (the
            // bash/IFS set); the one-sided forms are stripLeading/stripTrailing.
            let text = render_child(&node.text, ctx)?;
            let meth = match (node.leading, node.trailing) {
                (true, true) => "strip",
                (true, false) => "stripLeading",
                (false, true) => "stripTrailing",
                (false, false) => return Some(format!("String.valueOf({text})")),
            };
            Some(format!("String.valueOf({text}).{meth}()"))
        }
        Backend::Sh => None,
        Backend::Zig => {
            let text = render_child(&node.text, ctx)?;
            Some(format!("std.mem.trim(u8, {}, \" \\t\\n\")", text))
        }
        _ => None,
    }
}

// ── AffixStrip ──────────────────────────────────────────────────────

pub fn affix_strip(node: &AffixStrip, ctx: &ExprRenderCtx) -> Option<String> {
    match ctx.backend {
        Backend::Perl => {
            // ONE occurrence: test then strip, mirroring Go's
            // TrimPrefix/TrimSuffix (absent affix → text unchanged)
            let text = crate::ir::ir_expr_to_perl(&node.text);
            let pat = crate::ir::ir_expr_to_perl(&node.pattern);
            if node.prefix {
                Some(format!("(index({}, {}) == 0 ? substr({}, length({})) : {})",
                    text, pat, text, pat, text))
            } else {
                Some(format!("(substr({}, -length({})) eq {} ? substr({}, 0, length({}) - length({})) : {})",
                    text, pat, pat, text, text, pat, text))
            }
        }
        Backend::Estree => {
            let text = render_child(&node.text, ctx)?;
            let pat = render_child(&node.pattern, ctx)?;
            if node.prefix {
                Some(format!("({}.startsWith({}) ? {}.slice({}.length) : {})",
                    text, pat, text, pat, text))
            } else {
                Some(format!("({}.endsWith({}) ? {}.slice(0, {}.length - {}.length) : {})",
                    text, pat, text, text, pat, text))
            }
        }
        Backend::Go => {
            let text = render_child(&node.text, ctx)?;
            let pat = render_child(&node.pattern, ctx)?;
            if node.prefix {
                Some(format!("strings.TrimPrefix({}, {})", text, pat))
            } else {
                Some(format!("strings.TrimSuffix({}, {})", text, pat))
            }
        }
        Backend::Rust => {
            let text = render_child(&node.text, ctx)?;
            let pat = render_child(&node.pattern, ctx)?;
            if node.prefix {
                Some(format!("{}.strip_prefix({}).unwrap_or({})", text, pat, text))
            } else {
                Some(format!("{}.strip_suffix({}).unwrap_or({})", text, pat, text))
            }
        }
        Backend::Python => {
            // ONE occurrence, an absent affix leaves the text unchanged:
            // removeprefix/removesuffix are exactly Go's
            // TrimPrefix/TrimSuffix.
            let text = render_child(&node.text, ctx)?;
            let pat = render_child(&node.pattern, ctx)?;
            if node.prefix {
                Some(format!("str({text}).removeprefix(str({pat}))"))
            } else {
                Some(format!("str({text}).removesuffix(str({pat}))"))
            }
        }
        Backend::Java => {
            // Java has no "remove at most one occurrence": strip only when
            // present, else pass the text through.
            let text = render_child(&node.text, ctx)?;
            let pat = render_child(&node.pattern, ctx)?;
            if node.prefix {
                Some(format!(
                    "(String.valueOf({t}).startsWith({p}) ? String.valueOf({t}).substring(({p}).length()) : String.valueOf({t}))",
                    t = text, p = pat))
            } else {
                Some(format!(
                    "(String.valueOf({t}).endsWith({p}) ? String.valueOf({t}).substring(0, String.valueOf({t}).length() - ({p}).length()) : String.valueOf({t}))",
                    t = text, p = pat))
            }
        }
        Backend::Sh => None,
        Backend::Zig => None,
        _ => None,
    }
}

// ── RegexpFind ───────────────────────────────────────────────────────

pub fn regexp_find(node: &RegexpFind, ctx: &ExprRenderCtx) -> Option<String> {
    match ctx.backend {
        Backend::Perl => {
            // leftmost match via a capturing group; "" when no match
            let text = crate::ir::ir_expr_to_perl(&node.text);
            let pat = crate::ir::ir_expr_to_perl(&node.pattern);
            Some(format!("do {{ my $m; ({} =~ /({})/) && ($m = $1); $m // \"\" }}", text, pat))
        }
        Backend::Estree => {
            let text = render_child(&node.text, ctx)?;
            let pat = render_child(&node.pattern, ctx)?;
            Some(format!("(({}).match(new RegExp({})) || [\"\"])[0]", text, pat))
        }
        Backend::Python => {
            // `grep -o`-ish leftmost match: "" when there is no match.
            let text = render_child(&node.text, ctx)?;
            let pat = render_child(&node.pattern, ctx)?;
            Some(format!(
                "(lambda _m: _m.group(0) if _m else \"\")(re.search({p}, str({text})))",
                p = pat, text = text))
        }
        _ => None,
    }
}

// ── CgoCall ──────────────────────────────────────────────────────────

pub fn cgo_call(node: &CgoCall, ctx: &ExprRenderCtx) -> Option<String> {
    match ctx.backend {
        // The faithful execution of a cgo-bound call is the NATIVE call
        // — only the C frontend build can provide it. Perl/JS render a
        // loud runtime failure, never silence.
        Backend::Perl => Some(format!(
            "die \"otranspilerl: cgo-path construct {} requires the C frontend build\\n\";",
            node.target
        )),
        _ => None,
    }
}

// ── CutsetTrim ──────────────────────────────────────────────────────

pub fn cutset_trim(node: &CutsetTrim, ctx: &ExprRenderCtx) -> Option<String> {
    match ctx.backend {
        Backend::Perl => {
            // strip ALL leading/trailing chars present in the cutset;
            // \Q..\E quotes the set so metachars stay literal.
            // side: "left" = leading only, "right" = trailing only
            // (Go strings.TrimLeft / strings.TrimRight).
            let text = crate::ir::ir_expr_to_perl(&node.text);
            let cut = crate::ir::ir_expr_to_perl(&node.cutset);
            let strip_leading = node.side.as_deref() != Some("right");
            let strip_trailing = node.side.as_deref() != Some("left");
            let mut body = String::from("do { my $t = ");
            body.push_str(&text);
            body.push_str("; my $c = ");
            body.push_str(&cut);
            if strip_leading {
                body.push_str("; $t =~ s/^[\\Q$c\\E]+//");
            }
            if strip_trailing {
                body.push_str("; $t =~ s/[\\Q$c\\E]+$//");
            }
            body.push_str("; $t }");
            Some(body)
        }
        Backend::Estree => None,
        Backend::Go => {
            let text = render_child(&node.text, ctx)?;
            let cut = render_child(&node.cutset, ctx)?;
            match node.side.as_deref() {
                Some("left") => Some(format!("strings.TrimLeft({}, {})", text, cut)),
                Some("right") => Some(format!("strings.TrimRight({}, {})", text, cut)),
                _ => Some(format!("strings.Trim({}, {})", text, cut)),
            }
        }
        Backend::Rust => {
            let text = render_child(&node.text, ctx)?;
            let cut = render_child(&node.cutset, ctx)?;
            Some(format!("{}.trim_matches(|c| {}.contains(c))", text, cut))
        }
        Backend::Python => {
            // python's strip(chars) takes a CHARACTER SET (like Go's Trim);
            // lstrip/rstrip are the one-sided forms.
            let text = render_child(&node.text, ctx)?;
            let cut = render_child(&node.cutset, ctx)?;
            let meth = match node.side.as_deref() {
                Some("left") => "lstrip",
                Some("right") => "rstrip",
                _ => "strip",
            };
            Some(format!("str({text}).{meth}(str({cut}))"))
        }
        Backend::Java => {
            // CutsetTrim over a CHARACTER SET (Go's Trim family): one
            // shared helper covers both/left/right.
            let text = render_child(&node.text, ctx)?;
            let cut = render_child(&node.cutset, ctx)?;
            let side = match node.side.as_deref() {
                Some(s @ ("left" | "right")) => java_str_lit(s),
                _ => "null".to_string(),
            };
            Some(format!("shCutsetTrim(String.valueOf({text}), String.valueOf({cut}), {side})"))
        }
        Backend::Sh => None,
        _ => None,
    }
}

// ── RepeatStr ────────────────────────────────────────────────────────

pub fn repeat_str(node: &RepeatStr, ctx: &ExprRenderCtx) -> Option<String> {
    match ctx.backend {
        Backend::Perl => {
            let text = crate::ir::ir_expr_to_perl(&node.text);
            let count = crate::ir::ir_expr_to_perl(&node.count);
            Some(format!("{} x {}", text, count))
        }
        Backend::Estree => {
            let text = render_child(&node.text, ctx)?;
            let count = render_child(&node.count, ctx)?;
            Some(format!("{}.repeat({})", text, count))
        }
        Backend::Go => {
            let text = render_child(&node.text, ctx)?;
            let count = render_child(&node.count, ctx)?;
            Some(format!("strings.Repeat({}, {})", text, count))
        }
        Backend::Rust => {
            let text = render_child(&node.text, ctx)?;
            let count = render_child(&node.count, ctx)?;
            Some(format!("{}.repeat({} as usize)", text, count))
        }
        Backend::C => None,
        Backend::Python => {
            // `t * n` is python's repeat; an int() guard makes a numeric
            // shell var usable and a negative count yields "" (as in JS).
            let text = render_child(&node.text, ctx)?;
            let count = render_child(&node.count, ctx)?;
            Some(format!("(str({text}) * int({count}))"))
        }
        Backend::Java => {
            let text = render_child(&node.text, ctx)?;
            let count = render_child(&node.count, ctx)?;
            Some(format!("String.valueOf({text}).repeat((int) ({count}))"))
        }
        Backend::Sh => None,
        Backend::Zig => Some(format!("/* TODO: RepeatStr Zig */")),
        _ => None,
    }
}

// ── CaseTransform ────────────────────────────────────────────────────

pub fn case_transform(node: &CaseTransform, ctx: &ExprRenderCtx) -> Option<String> {
    match ctx.backend {
        Backend::Perl => {
            let text = crate::ir::ir_expr_to_perl(&node.text);
            if node.upper { Some(format!("uc({})", text)) } else { Some(format!("lc({})", text)) }
        }
        Backend::Estree => {
            let text = render_child(&node.text, ctx)?;
            if node.upper { Some(format!("{}.toUpperCase()", text)) } else { Some(format!("{}.toLowerCase()", text)) }
        }
        Backend::Go => {
            let text = render_child(&node.text, ctx)?;
            if node.upper { Some(format!("strings.ToUpper({})", text)) } else { Some(format!("strings.ToLower({})", text)) }
        }
        Backend::Rust => {
            let text = render_child(&node.text, ctx)?;
            if node.upper { Some(format!("{}.to_uppercase()", text)) } else { Some(format!("{}.to_lowercase()", text)) }
        }
        Backend::C => None,
        Backend::Python => {
            let text = render_child(&node.text, ctx)?;
            if node.upper {
                Some(format!("str({text}).upper()"))
            } else {
                Some(format!("str({text}).lower()"))
            }
        }
        Backend::Java => {
            let text = render_child(&node.text, ctx)?;
            if node.upper {
                Some(format!("String.valueOf({text}).toUpperCase()"))
            } else {
                Some(format!("String.valueOf({text}).toLowerCase()"))
            }
        }
        Backend::Sh => {
            // ${v^^} / ${v,,} — bash 4 parameter expansion, the source
            // construct for the `${var^^}` lowering.
            let text = render_child(&node.text, ctx)?;
            if node.upper {
                Some(format!("${{{}^^}}", text))
            } else {
                Some(format!("${{{}..}}", text))
            }
        }
        Backend::Zig => Some(format!("/* TODO: CaseTransform Zig */")),
        _ => None,
    }
}

// ── PathName ─────────────────────────────────────────────────────────

pub fn path_name(node: &PathName, ctx: &ExprRenderCtx) -> Option<String> {
    let which = if node.which == "dirname" { "dirname" } else { "basename" };
    // EXACT GNU basename/dirname semantics, verified against bash on
    // trailing-slash, all-slash, no-slash and multi-slash inputs.
    //
    // The naive forms — perl `s|.*/||`, JS `split("/").pop()` — are WRONG
    // for `a/b/` (they yield "" instead of "b") and for dirname of a
    // pathless word (`abc` → "" instead of "."). Corpus scripts do pass
    // trailing-slash paths, so both were live bugs.
    //
    //   strip trailing '/'s while len > 1
    //   basename: an all-slash input → "/"; else after the last '/'
    //   dirname : no '/' → "."; else the prefix, re-stripped, "" → "/"
    match ctx.backend {
        Backend::Perl => {
            let text = crate::ir::ir_expr_to_perl(&node.text);
            if which == "basename" {
                Some(format!(
                    "do {{ my $p = {text}; while (length($p) > 1 && substr($p, -1) eq \"/\") {{ chop $p }} \
if ($p eq \"/\") {{ \"/\" }} else {{ my $i = rindex($p, \"/\"); $i < 0 ? $p : substr($p, $i + 1) }} }}"
                ))
            } else {
                Some(format!(
                    "do {{ my $p = {text}; while (length($p) > 1 && substr($p, -1) eq \"/\") {{ chop $p }} \
my $i = rindex($p, \"/\"); if ($i < 0) {{ \".\" }} else {{ my $d = substr($p, 0, $i); \
while (length($d) > 1 && substr($d, -1) eq \"/\") {{ chop $d }} $d eq \"\" ? \"/\" : $d }} }}"
                ))
            }
        }
        Backend::Estree => {
            // The non-text-ops path already lowers `${p##*/}` to this
            // correct shape natively; the handler must match it.
            let text = render_child(&node.text, ctx)?;
            let s = format!("String({text})");
            if which == "basename" {
                Some(format!(
                    "(function(){{ let s = {s}.replace(/\\/+$/, \"\"); if (s === \"\") s = \"/\"; \
if (s === \"/\") return \"/\"; const i = s.lastIndexOf(\"/\"); return i < 0 ? s : s.slice(i + 1); }})()"
                ))
            } else {
                Some(format!(
                    "(function(){{ let s = {s}; while (s.length > 1 && s.endsWith(\"/\")) s = s.slice(0, -1); \
const i = s.lastIndexOf(\"/\"); if (i < 0) return \".\"; \
let d = s.slice(0, i); while (d.length > 1 && d.endsWith(\"/\")) d = d.slice(0, -1); \
return d === \"\" ? \"/\" : d; }})()"
                ))
            }
        }
        Backend::Python => {
            // posixpath IS the stdlib's exact GNU implementation — the
            // natural leaf (no hand-rolled edge-case set to get wrong).
            let text = render_child(&node.text, ctx)?;
            if which == "basename" {
                Some(format!("__import__(\"posixpath\").basename(str({text}))"))
            } else {
                Some(format!("__import__(\"posixpath\").dirname(str({text}))"))
            }
        }
        Backend::Java => {
            // Java has no GNU basename/dirname in the stdlib: ONE shared
            // helper keeps the semantics verified in one place.
            let text = render_child(&node.text, ctx)?;
            if which == "basename" {
                Some(format!("shBasename(String.valueOf({text}))"))
            } else {
                Some(format!("shDirname(String.valueOf({text}))"))
            }
        }
        Backend::Sh => {
            // The source construct itself — exact by construction.
            let text = render_child(&node.text, ctx)?;
            if which == "basename" {
                Some(format!("$(basename -- {text})"))
            } else {
                Some(format!("$(dirname -- {text})"))
            }
        }
        Backend::Go => None,
        Backend::Zig => None,
        _ => None,
    }
}

// ── Split ────────────────────────────────────────────────────────────

pub fn split(node: &Split, ctx: &ExprRenderCtx) -> Option<String> {
    let text = render_child(&node.text, ctx)?;
    match ctx.backend {
        Backend::Perl => {
            if node.is_regex {
                Some(format!("split(/{}/, {}, -1)", node.delim, text))
            } else {
                let d = node.delim.replace('\'', "''");
                Some(format!("split('{}', {}, -1)", d, text))
            }
        }
        Backend::Estree => {
            if node.is_regex {
                Some(format!("{}.split(/{}/)", text, node.delim))
            } else {
                Some(format!("{}.split('{}')", text, node.delim.replace('\'', "\\'")))
            }
        }
        Backend::Go => Some(format!("strings.Split({}, \"{}\" /* TODO */)", text, node.delim)),
        Backend::Rust => Some(format!("{}.split('{}')", text, node.delim)),
        Backend::C => None,
        Backend::Python => {
            // python's split takes a LITERAL separator (like Perl's
            // `split 'lit'`), so a non-regex Split is exact. A regex split
            // has no non-re module equivalent — refuse rather than guess.
            if node.is_regex {
                return None;
            }
            Some(format!("str({text}).split({})", py_str_lit(&node.delim)))
        }
        Backend::Java => {
            if node.is_regex {
                return None;
            }
            Some(format!("String.valueOf({text}).split(java.util.regex.Pattern.quote({}), -1)",
                java_str_lit(&node.delim)))
        }
        Backend::Sh => None,
        Backend::Zig => Some(format!("std.mem.splitScalar(u8, {}, '{}')", text, node.delim)),
        _ => None,
    }
}

// ── WordCount ────────────────────────────────────────────────────────

pub fn word_count(node: &WordCount, ctx: &ExprRenderCtx) -> Option<String> {
    let text = render_child(&node.text, ctx)?;
    match ctx.backend {
        Backend::Perl => {
            Some(format!("scalar(grep {{ length }} split(/\\\\s+/, {}, -1))", text))
        }
        Backend::Estree => {
            Some(format!("({}).split(/\\s+/).filter(w => w).length", text))
        }
        Backend::Go => Some(format!("len(strings.Fields({}))", text)),
        Backend::Rust => Some(format!("{}.split_whitespace().count()", text)),
        // Python: str.split() with no argument splits on RUNS of
        // whitespace and drops leading/trailing empties — exactly `wc -w`.
        Backend::Python => Some(format!("len(str({text}).split())")),
        // Java: the same transition-count helper the C backend uses (a
        // materialised split would allocate and get edge cases wrong).
        Backend::Java => Some(format!("shWcWords(String.valueOf({text}))")),
        Backend::Sh => None,
        // C: inline transition-count scan — no allocation (the natural
        // wc -w rendering; a materialised split() is the JS/Perl idiom)
        Backend::C => Some(format!("_sh_wc_words({})", text)),
        _ => None,
    }
}

// ── ArrayLen ─────────────────────────────────────────────────────────

pub fn array_len(node: &ArrayLen, ctx: &ExprRenderCtx) -> Option<String> {
    let arr = render_child(&node.array, ctx)?;
    match ctx.backend {
        Backend::Perl => Some(format!("scalar({})", arr)),
        Backend::Estree => Some(format!("{}.length", arr)),
        Backend::Go => Some(format!("len({})", arr)),
        Backend::Rust => Some(format!("{}.len()", arr)),
        Backend::C => None,
        Backend::Python => Some(format!("len({})", arr)),
        Backend::Java => Some(format!("(long) ({}.length)", arr)),
        Backend::Sh => None,
        Backend::Zig => Some(format!("{}.len", arr)),
        _ => None,
    }
}

// ── RegCount ─────────────────────────────────────────────────────────

pub fn reg_count(node: &RegCount, ctx: &ExprRenderCtx) -> Option<String> {
    let text = render_child(&node.text, ctx)?;
    match ctx.backend {
        Backend::Perl => Some(format!("() = ({} =~ /{}/g)", text, node.pattern)),
        Backend::Estree => Some(format!("({}.match(/{}/g) || []).length", text, node.pattern)),
        Backend::Go => Some(format!("len(regexp.MustCompile(\"{}\").FindAllStringIndex({}, -1))", node.pattern, text)),
        Backend::Rust => Some(format!("{}.match(/{}/g) /* TODO */", text, node.pattern)),
        Backend::C => None,
        Backend::Python => Some(format!(
            "len(re.findall({}, str({})))", py_str_lit(&node.pattern), text)),
        Backend::Java => Some(format!(
            "shRegCount(String.valueOf({}), {})", text, java_str_lit(&node.pattern))),
        Backend::Sh => None,
        Backend::Zig => Some(format!("/* TODO: RegCount Zig */")),
        _ => None,
    }
}
