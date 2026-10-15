//! Handler for FieldExtract — renders `cut -dD -fF` natively per backend.
//!
//! Drop-in: add this file → build.rs picks it up → dispatch works.
//! Remove it → falls back to sh2.fieldExtract().

use crate::shir_nodes::FieldExtract;
use crate::render_ext_expr::{ExprRenderCtx, Backend};

pub(crate) fn render(node: &FieldExtract, ctx: &ExprRenderCtx) -> Option<String> {
    // Complex cases fall back to sh2.*
    if node.suppress_no_delim || node.output_delimiter.is_some() {
        return None;
    }

    // Build the field index list (1-indexed in shell → 0-indexed)
    let field_indices: Vec<u32> = node.fields.iter().filter_map(|f| {
        match f {
            crate::ir::FieldRange::Single(n) => Some(n - 1),
            _ => None, // ranges fall back
        }
    }).collect();

    if field_indices.is_empty() {
        return None;
    }

    match ctx.backend {
        Backend::Perl => render_perl(node, &field_indices),
        Backend::Python => render_python(node, &field_indices),
        Backend::Java => render_java(node, &field_indices),
        Backend::Sh => render_sh(node, &field_indices),
        _ => None, // unknown backend → fall back to sh2.*
    }
}

/// GNU `cut -d D -f LIST` semantics, exactly as the Perl arm implements
/// them (and verified against bash):
///  * the delimiter is a LITERAL (not a regex)
///  * a line WITHOUT the delimiter passes through whole (no `-s`)
///  * a MISSING trailing field contributes NOTHING ("d:e" -f1,3 → "d")
///  * the selection is ASCENDING regardless of the flag order
///
/// Python's `str.split(sep)` keeps empty fields and splits on the literal —
/// the exact match for `cut -d`.
fn render_python(node: &FieldExtract, indices: &[u32]) -> Option<String> {
    // `str()` guards a numeric operand; the child renderer is not available
    // in this leaf file, so a non-string operand refuses (the sh2.* stub
    // stays — never a wrong value).
    let text = match &node.text {
        crate::ir::IrExpr::Str(s, _) => super::all_nodes::py_str_lit(s),
        _ => return None,
    };
    let d = super::all_nodes::py_str_lit(&node.delimiter);
    if indices.len() == 1 {
        let i = indices[0];
        // A missing trailing field yields "" (cut prints nothing for it).
        Some(format!(
            "((lambda _p: (_p[{i}] if {i} < len(_p) else \"\"))({text}.split({d})))"
        ))
    } else {
        let idx_list: String = indices
            .iter()
            .map(|i| i.to_string())
            .collect::<Vec<_>>()
            .join(", ");
        // Keep only the indices the line has, then rejoin with the SAME
        // delimiter — the ascending, drop-missing behaviour of GNU cut.
        Some(format!(
            "((lambda _p: {d}.join(_p[_i] for _i in ({idx_list},) if _i < len(_p)))({text}.split({d})))"
        ))
    }
}

/// Java: the same semantics via ONE shared runtime helper (no per-call
/// slicing subtleties, one verified implementation).
fn render_java(node: &FieldExtract, indices: &[u32]) -> Option<String> {
    let text = match &node.text {
        crate::ir::IrExpr::Str(s, _) => super::all_nodes::java_str_lit(s),
        _ => return None,
    };
    let idx_list: String = indices
        .iter()
        .map(|i| i.to_string())
        .collect::<Vec<_>>()
        .join(", ");
    Some(format!(
        "shCutField({text}, {}, new int[] {{ {idx_list} }})",
        super::all_nodes::java_str_lit(&node.delimiter)
    ))
}

/// sh: the source construct itself (`cut -d D -f LIST`) — exact by
/// construction and the cheapest form on this target.
fn render_sh(node: &FieldExtract, indices: &[u32]) -> Option<String> {
    // 1-indexed in shell: the node stores 0-indexed, so render +1.
    let list: String = indices
        .iter()
        .map(|i| (i + 1).to_string())
        .collect::<Vec<_>>()
        .join(",");
    let text = match &node.text {
        crate::ir::IrExpr::Str(s, _) => super::all_nodes::sh_quote_arg(s),
        _ => return None,
    };
    Some(format!(
        "$(printf '%s' {text} | cut -d {} -f {list})",
        super::all_nodes::sh_quote_arg(&node.delimiter)
    ))
}

fn render_perl(node: &FieldExtract, indices: &[u32]) -> Option<String> {
    // NOTE: the result must not START with "(" — the statement emitter
    // renders `say {expr};`, and `say (split(..))[0]` parses as a function
    // call (syntax error). Wrap in do { … } which is always a term.
    let text = crate::ir::ir_expr_to_perl(&node.text);
    let delim = format!("'{}'", node.delimiter.replace('\'', "''"));

    if indices.len() == 1 {
        Some(format!(
            "do {{ my @_p = split({}, {}, -1); $_p[{}] }}",
            delim, text, indices[0]
        ))
    } else {
        // bash cut drops MISSING trailing fields ("d:e" -f1,3 → "d", not
        // "d:") and normalizes ascending — grep the indices to those the
        // line actually has before joining.
        let idx_list: String = indices.iter().map(|i| i.to_string()).collect::<Vec<_>>().join(", ");
        Some(format!(
            "do {{ my @_p = split({}, {}, -1); join({}, map {{ $_p[$_] }} grep {{ $_ <= $#_p }} ({})) }}",
            delim, text, delim, idx_list
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ir::{IrExpr, FieldRange};

    fn perl_ctx() -> ExprRenderCtx<'static> {
        ExprRenderCtx::bare(Backend::Perl, 0)
    }

    #[test]
    fn perl_single_field() {
        let node = FieldExtract {
            text: IrExpr::Var("csv".to_string(), None),
            delimiter: ",".to_string(),
            fields: vec![FieldRange::Single(2)],
            suppress_no_delim: false,
            output_delimiter: None,
        };
        let result = render(&node, &perl_ctx()).unwrap();
        assert!(result.contains("split"));
        assert!(result.contains("[1]")); // 0-indexed
    }

    #[test]
    fn perl_multi_field() {
        let node = FieldExtract {
            text: IrExpr::Var("csv".to_string(), None),
            delimiter: ",".to_string(),
            fields: vec![FieldRange::Single(1), FieldRange::Single(3)],
            suppress_no_delim: false,
            output_delimiter: None,
        };
        let result = render(&node, &perl_ctx()).unwrap();
        assert!(result.contains("join"));
        assert!(result.contains("split"));
    }

    #[test]
    fn suppress_falls_back() {
        let node = FieldExtract {
            text: IrExpr::Var("csv".to_string(), None),
            delimiter: ",".to_string(),
            fields: vec![FieldRange::Single(1)],
            suppress_no_delim: true,
            output_delimiter: None,
        };
        assert!(render(&node, &perl_ctx()).is_none());
    }

    #[test]
    fn unknown_backend_falls_back() {
        let node = FieldExtract {
            text: IrExpr::Var("csv".to_string(), None),
            delimiter: ",".to_string(),
            fields: vec![FieldRange::Single(1)],
            suppress_no_delim: false,
            output_delimiter: None,
        };
        let ctx = ExprRenderCtx::bare(Backend::Rust, 0);
        assert!(render(&node, &ctx).is_none(), "Rust falls back to sh2.*");
    }
}
