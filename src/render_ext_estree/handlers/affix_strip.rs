//! AffixStrip → native JS conditional: startsWith/endsWith test, then
//! slice off ONE occurrence of the affix (see affix_strip.node).

use crate::estree::Expr;
use crate::shir_nodes::AffixStrip;

pub(crate) fn render(node: &AffixStrip) -> Option<Expr> {
    let text = crate::shir::expr_to_estree_pub(&node.text);
    let pat = crate::shir::expr_to_estree_pub(&node.pattern);
    // pat.length — the byte/code-unit length the strip advances by
    let pat_len = Expr::MemberExpression {
        object: Box::new(pat.clone()),
        property: Box::new(Expr::Identifier {
            name: "length".to_string(),
        }),
        computed: false,
        optional: false,
    };
    let stripped = if node.prefix {
        // s.slice(p.length)
        crate::estree::method_call(text.clone(), "slice", vec![pat_len])
    } else {
        // s.slice(0, s.length - p.length)
        let own_len = Expr::MemberExpression {
            object: Box::new(text.clone()),
            property: Box::new(Expr::Identifier {
                name: "length".to_string(),
            }),
            computed: false,
            optional: false,
        };
        let start0 = Expr::Literal {
            value: serde_json::Value::from(0),
            raw: None,
            regex: None,
        };
        let end = Expr::BinaryExpression {
            operator: "-".to_string(),
            left: Box::new(own_len),
            right: Box::new(pat_len),
        };
        crate::estree::method_call(text.clone(), "slice", vec![start0, end])
    };
    // hasAffix ? stripped : text — single-occurrence semantics: when
    // the affix is absent the text flows through unchanged
    let test = if node.prefix {
        crate::estree::method_call(text.clone(), "startsWith", vec![pat])
    } else {
        crate::estree::method_call(text.clone(), "endsWith", vec![pat])
    };
    Some(Expr::ConditionalExpression {
        test: Box::new(test),
        consequent: Box::new(stripped),
        alternate: Box::new(text),
    })
}
