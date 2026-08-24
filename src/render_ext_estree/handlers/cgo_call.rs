//! CgoCall → `sh2.cgoUnsupported("target", args...)`: the JS/browser
//! path THROWS at runtime rather than silently mis-lowering (see
//! cgo_call.node — the faithful execution of these constructs is the
//! C frontend build).

use crate::estree::Expr;
use crate::shir_nodes::CgoCall;

pub(crate) fn render(node: &CgoCall) -> Option<Expr> {
    let mut args = vec![crate::estree::str_lit(&node.target)];
    for a in &node.args {
        args.push(crate::shir::expr_to_estree_pub(a));
    }
    Some(Expr::CallExpression {
        callee: Box::new(Expr::MemberExpression {
            object: Box::new(Expr::Identifier {
                name: "sh2".to_string(),
            }),
            property: Box::new(Expr::Identifier {
                name: "cgoUnsupported".to_string(),
            }),
            computed: false,
            optional: false,
        }),
        arguments: args,
        optional: false,
    })
}
