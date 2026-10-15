//! Store-backed array reads must keep their data: the `sh2.max("name")`
//! family.
//!
//! The py-sh-go frontend lowers `print(max(xs))` to a store-reading call
//! — `sh2.max("xs")` — whose variable is spelled as a STRING LITERAL, and
//! its `x = [3, 1, 4]` to a `sh2.setArray`. The estree pipeline then runs
//! two passes that each read only one half of that contract:
//!
//!   1. `lower_native_arrays` (estree.rs) folds a provably-static array's
//!      store calls into a native JS binding — `sh2.setArray("xs", […])`
//!      becomes `let xs = […]`. Its ref vocabulary covers arrayIndex /
//!      arrayLen / arrayItems / arrayValues, and NOT the reductions, so
//!      folding a name read by `sh2.max("xs")` left that call reading an
//!      EMPTY STORE.
//!   2. `drop_dead_top_decls` (estree.rs) removes a leading declaration
//!      whose name is never READ. Its read-set walk follows IDENTIFIERS,
//!      so `sh2.max("xs")` (a string literal) did not count as a read and
//!      the `let xs = […]` this pass now needed was dropped as dead.
//!
//! Together the two produced `print(max([3,1,4]))` → `0` — measured on
//! py-sh-go's t92_max_min_sum.py, whose six `max/min/sum` lines ALL
//! printed `0`, and on t93_extent_mixed.py and t88_factor53.py.
//!
//! This pins both halves at the emission boundary (the IrProgram → ESTree
//! JSON text), which is the cheapest place to see them: a fold that
//! fires, or a drop that fires, changes the JSON here even when the
//! frontend that produced the IrProgram is not involved.

use debashl::ir::{AssignTarget, IrExpr, IrProgram, IrStmt, StrStyle};

/// `xs = [1, 2, 3]` as the py frontend lowers it.
fn set_array_stmt(name: &str, items: &[&str]) -> IrStmt {
    IrStmt::Assign {
        targets: vec![AssignTarget {
            var: name.to_string(),
            indices: vec![],
            sigil: None,
        }],
        expr: IrExpr::Call {
            func: "setArray".to_string(),
            args: vec![
                IrExpr::Str(name.to_string(), StrStyle::DoubleQuoted),
                IrExpr::Array(
                    items
                        .iter()
                        .map(|s| IrExpr::Str(s.to_string(), StrStyle::DoubleQuoted))
                        .collect(),
                ),
            ],
        },
        asm: None,
    }
}

/// `print(<reduction>(name))` as the py frontend lowers it — the echo of a
/// store-reading call.
fn print_reduction_stmt(func: &str, name: &str) -> IrStmt {
    IrStmt::Expr(IrExpr::Call {
        func: "exec".to_string(),
        args: vec![
            IrExpr::Str("echo".to_string(), StrStyle::DoubleQuoted),
            IrExpr::Array(vec![IrExpr::Call {
                func: func.to_string(),
                args: vec![IrExpr::Str(name.to_string(), StrStyle::DoubleQuoted)],
            }]),
        ],
    })
}

fn program(stmts: Vec<IrStmt>) -> IrProgram {
    IrProgram {
        imports: vec![],
        requires: vec![],
        stmts,
        subs: vec![],
        var_types: vec![],
        stmt_lines: vec![],
        var_lengths: vec![],
        var_const: vec![],
        var_lifetimes: vec![],
        var_nospace: vec![],
        var_storage: vec![],
        var_bash_env: vec![],
    }
}

/// The core assertion, for every member of the store-reading family: the
/// call must be able to read the data, which means the store write it
/// depends on must SURVIVE. A `let xs = […]` with the `sh2.setArray`
/// gone is exactly the broken shape (the runtime store never learns
/// about `xs`), so the `setArray` call is the witness.
fn assert_store_survives(func: &str) {
    let json = debashl::shir::shir_to_estree_json(&program(vec![
        set_array_stmt("xs", &["1", "2", "3"]),
        print_reduction_stmt(func, "xs"),
    ]))
    .expect("estree json");
    assert!(
        json.contains("\"setArray\""),
        "sh2.{func}(\"xs\") must not lose the store write it reads — \
         the native-array fold (or the dead-decl drop) elided it: {json}"
    );
    assert!(
        json.contains(&format!("\"{func}\"")),
        "the sh2.{func} call itself must survive: {json}"
    );
}

#[test]
fn max_keeps_its_store_backed_array() {
    assert_store_survives("max");
}

#[test]
fn min_keeps_its_store_backed_array() {
    assert_store_survives("min");
}

#[test]
fn sum_keeps_its_store_backed_array() {
    assert_store_survives("sum");
}

/// `sortedIntJoin` / `sortedBigintJoin` reach the same guarantee by a
/// DIFFERENT route, and pinning that difference is what keeps the fix
/// from being over-broad.
///
/// These two are rewritten by a later pass (`array_migrate_program`,
/// shir.rs) into an inline map/sort/join IIFE over a native binding, so
/// the fold firing is fine — the array is not stranded, it moved.
/// `max`/`min`/`sum` have no such rewrite, which is exactly why the fold
/// must refuse for them (the tests above).
///
/// The assertion is therefore the INVARIANT rather than one mechanism:
/// a store-reading call must never be left with neither a store write
/// nor a native binding to read. That is the t92/t93 bug shape, and it
/// is the property worth pinning — not which of the two routes the
/// pipeline happens to take.
#[test]
fn joins_never_strand_the_array() {
    for func in ["sortedIntJoin", "sortedBigintJoin"] {
        let json = debashl::shir::shir_to_estree_json(&program(vec![
            set_array_stmt("xs", &["1", "2", "3"]),
            print_reduction_stmt(func, "xs"),
        ]))
        .expect("estree json");
        let store_kept = json.contains("\"setArray\"");
        let has_binding = json.contains("\"VariableDeclaration\"");
        assert!(
            store_kept || has_binding,
            "sh2.{func}(\"xs\") has neither a store write nor a native \
             binding — the array is stranded: {json}"
        );
        // and a rewritten call must not ALSO linger as a store read
        if !store_kept {
            assert!(
                !json.contains(&format!("\"name\":\"{func}\"")),
                "the store-reading sh2.{func} call must be rewritten away \
                 once the array is native: {json}"
            );
        }
    }
}

/// The counterweight: a name whose ONLY consumer is a natively-rewritten
/// read must still FOLD. Without this the fix could be "refuse to fold
/// anything", which passes the tests above and silently gives back the
/// native-array lowering the pipeline exists to do.
#[test]
fn an_element_read_only_array_still_folds() {
    let json = debashl::shir::shir_to_estree_json(&program(vec![
        set_array_stmt("xs", &["1", "2", "3"]),
        // `print(xs[1])` — arrayIndex is in the fold's vocabulary
        IrStmt::Expr(IrExpr::Call {
            func: "exec".to_string(),
            args: vec![
                IrExpr::Str("echo".to_string(), StrStyle::DoubleQuoted),
                IrExpr::Array(vec![IrExpr::Call {
                    func: "arrayIndex".to_string(),
                    args: vec![
                        IrExpr::Str("xs".to_string(), StrStyle::DoubleQuoted),
                        IrExpr::Str("1".to_string(), StrStyle::DoubleQuoted),
                    ],
                }]),
            ],
        }),
    ]))
    .expect("estree json");
    assert!(
        !json.contains("\"setArray\""),
        "a plain element read must still fold to a native array: {json}"
    );
    assert!(
        json.contains("\"VariableDeclaration\""),
        "the folded binding must be a `let`: {json}"
    );
}
