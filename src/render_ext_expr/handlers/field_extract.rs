//! Handler for FieldExtract — renders `cut -dD -fF` as native Perl.
//!
//! Drop-in: add this file → build.rs picks it up → dispatch works.
//! Remove it → falls back to sh2.fieldExtract().

use crate::shir_nodes::FieldExtract;
use crate::shir_nodes::ExtExpr;
use crate::render_ext_expr::ExprRenderCtx;

pub(crate) fn render(node: &FieldExtract, ctx: &ExprRenderCtx) -> Option<String> {
    let text = crate::ir::ir_expr_to_perl(&node.text);

    // Simple case: single field, no output delimiter override, no -s
    if node.suppress_no_delim || node.output_delimiter.is_some() {
        // Complex case — fall back to sh2.fieldExtract(...)
        return None;
    }

    // Build the field index list (1-indexed in shell → 0-indexed in Perl)
    // For now, handle the simple case: single field
    // TODO: handle multi-field and ranges
    let field_indices: Vec<u32> = node.fields.iter().filter_map(|f| {
        match f {
            crate::ir::FieldRange::Single(n) => Some(n - 1), // 0-index for Perl
            _ => None, // ranges fall back
        }
    }).collect();

    if field_indices.is_empty() {
        return None;
    }

    // Simple single-field case: (split(delimiter, text, -1))[index]
    if field_indices.len() == 1 {
        let idx = field_indices[0];
        let delim = crate::ir::ir_expr_to_perl(&crate::ir::IrExpr::Str(
            node.delimiter.clone(),
            crate::ir::StrStyle::SingleQuoted,
        ));
        return Some(format!("(split({}, {}, -1))[{}]", delim, text, idx));
    }

    // Multi-field: join with delimiter
    let indices: String = field_indices.iter()
        .map(|i| format!("$_[{}]", i))
        .collect::<Vec<_>>()
        .join(", ");
    let delim = crate::ir::ir_expr_to_perl(&crate::ir::IrExpr::Str(
        node.delimiter.clone(),
        crate::ir::StrStyle::SingleQuoted,
    ));
    Some(format!("join({}, (split({}, {}, -1))[{}])", delim, delim, text, indices))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ir::{IrExpr, FieldRange};

    #[test]
    fn single_field_extract() {
        let node = FieldExtract {
            text: IrExpr::Var("csv".to_string(), None),
            delimiter: ",".to_string(),
            suppress_no_delim: false,
            output_delimiter: None,
            fields: vec![FieldRange::Single(2)], // -f2 → index 1
        };
        let ctx = ExprRenderCtx { indent: 0 };
        let result = render(&node, &ctx).unwrap();
        assert!(result.contains("split"));
        assert!(result.contains("[1]"));
    }

    #[test]
    fn multi_field_extract() {
        let node = FieldExtract {
            text: IrExpr::Var("csv".to_string(), None),
            delimiter: ",".to_string(),
            suppress_no_delim: false,
            output_delimiter: None,
            fields: vec![FieldRange::Single(1), FieldRange::Single(3)],
        };
        let ctx = ExprRenderCtx { indent: 0 };
        let result = render(&node, &ctx).unwrap();
        assert!(result.contains("join"));
        assert!(result.contains("split"));
    }

    #[test]
    fn suppress_no_delim_falls_back() {
        let node = FieldExtract {
            text: IrExpr::Var("csv".to_string(), None),
            delimiter: ",".to_string(),
            suppress_no_delim: true, // -s flag
            output_delimiter: None,
            fields: vec![FieldRange::Single(1)],
        };
        let ctx = ExprRenderCtx { indent: 0 };
        assert!(render(&node, &ctx).is_none(), "suppress -s falls back to sh2.*");
    }
}
