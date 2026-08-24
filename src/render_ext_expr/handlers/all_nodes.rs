//! All-in-one handler for semantic IR expression nodes.
//!
//! One file, one match per node × backend. build.rs picks up the
//! individual handler files, but this file demonstrates the pattern
//! for adding rendering to any backend.
//!
//! The Perl backend calls `ir_expr_to_perl()` on child expressions.
//! Other backends fall back to sh2.* for now — add native rendering
//! by matching on `ctx.backend` and emitting the target language.

use crate::shir_nodes::*;
use crate::shir_nodes::ExtExpr;
use crate::render_ext_expr::{ExprRenderCtx, Backend};

/// Render a child expression per-backend (all backends, not just Perl).
fn render_text(ctx: &ExprRenderCtx, expr: &crate::ir::IrExpr) -> Option<String> {
    match ctx.backend {
        Backend::Perl => Some(crate::ir::ir_expr_to_perl(expr)),
        _ => None, // placeholder until per-backend expr renderers exist
    }
}


// Helper: render a child expression for the current backend
fn render_child(expr: &crate::ir::IrExpr, ctx: &ExprRenderCtx) -> Option<String> {
    match ctx.backend {
        Backend::Perl => Some(crate::ir::ir_expr_to_perl(expr)),
        _ => None, // other backends: fall back to sh2.*
    }
}

// ── CharTranslate ────────────────────────────────────────────────────

pub fn char_translate(node: &CharTranslate, ctx: &ExprRenderCtx) -> Option<String> {
    match ctx.backend {
        Backend::Perl => {
            let text = crate::ir::ir_expr_to_perl(&node.text);
            if node.delete {
                Some(format!("do {{ my $t = {}; $t =~ tr/{}/d; $t }}", text, node.from))
            } else if node.squeeze {
                Some(format!("do {{ my $t = {}; my $f = '{}'; my $t2 = '{}'; $t =~ tr/$f/$t2/; $t =~ s/([$f])\\1+/$1/g; $t }}",
                    text, node.from.replace('\'', "''"), node.to.replace('\'', "''")))
            } else {
                Some(format!("do {{ my $t = {}; $t =~ tr/{}/{}; $t }}",
                    text, node.from.replace('\'', "''"), node.to.replace('\'', "''")))
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
        Backend::Zig => Some(format!("/* TODO: CharTranslate Zig */")),
        _ => None,
    }
}

// ── RegSub ───────────────────────────────────────────────────────────

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
            Some(format!("{}.replace(/{}/{}, \"{}\")", text, node.pattern, flag, node.replacement.replace('"', "\\\"")))
        }
        Backend::Go => Some(format!("/* TODO: RegSub Go */")),
        Backend::Rust => Some(format!("/* TODO: RegSub Rust */")),
        Backend::C => None,
        Backend::Zig => Some(format!("/* TODO: RegSub Zig */")),
        _ => None,
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
            "die \"debashc: cgo-path construct {} requires the C frontend build\\n\";",
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
            // \Q..\E quotes the set so metachars stay literal
            let text = crate::ir::ir_expr_to_perl(&node.text);
            let cut = crate::ir::ir_expr_to_perl(&node.cutset);
            Some(format!(
                "do {{ my $t = {}; my $c = {}; $t =~ s/^[\\Q$c\\E]+//; $t =~ s/[\\Q$c\\E]+$//; $t }}",
                text, cut
            ))
        }
        Backend::Estree => None,
        Backend::Go => {
            let text = render_child(&node.text, ctx)?;
            let cut = render_child(&node.cutset, ctx)?;
            Some(format!("strings.Trim({}, {})", text, cut))
        }
        Backend::Rust => {
            let text = render_child(&node.text, ctx)?;
            let cut = render_child(&node.cutset, ctx)?;
            Some(format!("{}.trim_matches(|c| {}.contains(c))", text, cut))
        }
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
        Backend::Zig => Some(format!("/* TODO: CaseTransform Zig */")),
        _ => None,
    }
}

// ── PathName ─────────────────────────────────────────────────────────

pub fn path_name(node: &PathName, ctx: &ExprRenderCtx) -> Option<String> {
    let which = if node.which == "dirname" { "dirname" } else { "basename" };
    match ctx.backend {
        Backend::Perl => {
            let text = crate::ir::ir_expr_to_perl(&node.text);
            if which == "basename" {
                Some(format!("do {{ my $p = {}; $p =~ s|.*/||; $p }}", text))
            } else {
                Some(format!("do {{ my $p = {}; $p =~ s|/[^/]*$||; $p eq \"\" ? \"/\" : $p }}", text))
            }
        }
        _ => None,
    }
}

// ── Split ────────────────────────────────────────────────────────────

pub fn split(node: &Split, ctx: &ExprRenderCtx) -> Option<String> {
    let text = render_text(ctx, &node.text)?;
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
        Backend::Zig => Some(format!("std.mem.splitScalar(u8, {}, '{}')", text, node.delim)),
        _ => None,
    }
}

// ── ArrayLen ─────────────────────────────────────────────────────────

pub fn array_len(node: &ArrayLen, ctx: &ExprRenderCtx) -> Option<String> {
    let arr = render_text(ctx, &node.array)?;
    match ctx.backend {
        Backend::Perl => Some(format!("scalar({})", arr)),
        Backend::Estree => Some(format!("{}.length", arr)),
        Backend::Go => Some(format!("len({})", arr)),
        Backend::Rust => Some(format!("{}.len()", arr)),
        Backend::C => None,
        Backend::Zig => Some(format!("{}.len", arr)),
        _ => None,
    }
}

// ── RegCount ─────────────────────────────────────────────────────────

pub fn reg_count(node: &RegCount, ctx: &ExprRenderCtx) -> Option<String> {
    let text = render_text(ctx, &node.text)?;
    match ctx.backend {
        Backend::Perl => Some(format!("() = ({} =~ /{}/g)", text, node.pattern)),
        Backend::Estree => Some(format!("({}.match(/{}/g) || []).length", text, node.pattern)),
        Backend::Go => Some(format!("len(regexp.MustCompile(\"{}\").FindAllStringIndex({}, -1))", node.pattern, text)),
        Backend::Rust => Some(format!("{}.match(/{}/g) /* TODO */", text, node.pattern)),
        Backend::C => None,
        Backend::Zig => Some(format!("/* TODO: RegCount Zig */")),
        _ => None,
    }
}
