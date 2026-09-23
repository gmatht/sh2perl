use debashl::ir::{IrProgram, IrStmt, IrExpr, StrStyle};

fn prog() -> Vec<IrStmt> {
    vec![
        IrStmt::Redirect {
            inner: vec![IrStmt::Expr(IrExpr::Call {
                func: "exec".into(),
                args: vec![IrExpr::Str("echo".into(), StrStyle::DoubleQuoted),
                           IrExpr::Array(vec![IrExpr::Str("data".into(), StrStyle::DoubleQuoted)])],
            })],
            redirects: vec![debashl::ir::IrRedirect {
                fd: Some(1), mode: "w".into(),
                target: IrExpr::Str("f".into(), StrStyle::DoubleQuoted),
                interpolate: true,
            }],
        },
        IrStmt::Expr(IrExpr::Call {
            func: "exec".into(),
            args: vec![IrExpr::Str("echo".into(), StrStyle::DoubleQuoted),
                       IrExpr::Array(vec![IrExpr::Str("wrote".into(), StrStyle::DoubleQuoted)])],
        }),
    ]
}

fn main() {
    for (name, _) in debashl::transforms::all() {
        std::env::set_var("SH2_TRANSFORMS", name);
        let mut s = prog();
        let c = debashl::transforms::apply(&mut s);
        println!("== {name}: changed={c}");
        if c { println!("   {:?}", s); }
    }
}
