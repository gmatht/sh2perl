//! Argv → `[sh2.argv0].concat(sh2.positional)`: the program name first,
//! then the arguments, as ONE array value (see argv.node). Concatenation is
//! a fresh array, so a later write to the caller's binding cannot reach the
//! runtime's positional store.

use crate::estree::Expr;
use crate::shir_nodes::Argv;

fn member(name: &str) -> Expr {
    Expr::MemberExpression {
        object: Box::new(Expr::Identifier {
            name: "sh2".to_string(),
        }),
        property: Box::new(Expr::Identifier {
            name: name.to_string(),
        }),
        computed: false,
        optional: false,
    }
}

pub(crate) fn render(_node: &Argv) -> Option<Expr> {
    Some(Expr::CallExpression {
        callee: Box::new(Expr::MemberExpression {
            object: Box::new(Expr::ArrayExpression {
                elements: vec![Some(member("argv0"))],
            }),
            property: Box::new(Expr::Identifier {
                name: "concat".to_string(),
            }),
            computed: false,
            optional: false,
        }),
        arguments: vec![member("positional")],
        optional: false,
    })
}
