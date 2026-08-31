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
        _ => None, // unknown backend → fall back to sh2.*
    }
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

    fn perl_ctx() -> ExprRenderCtx {
        ExprRenderCtx { backend: Backend::Perl, indent: 0 }
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
        let ctx = ExprRenderCtx { backend: Backend::Rust, indent: 0 };
        assert!(render(&node, &ctx).is_none(), "Rust falls back to sh2.*");
    }
}
