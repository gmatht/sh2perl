// GENERATED from sh2perl/src/ir.rs @ f68b2adfa407
// DO NOT EDIT — run harness/verus-sync-foldarith.sh
// Region: `enum ArithAst` and `fn fold_arith_ast` — the recursive Pass 0.5
// driver that runs on EVERY program.
//
// The block below is the RUNNING source text, mechanically transcribed, and
// this file is what Verus verifies: there is no hand copy of the recursion to
// drift from the code. Mechanical rewrites (the complete list):
//   * `&ArithAst` value parameter, `open spec fn`, `decreases ast`;
//   * `String` -> `Str`, `IrType` -> `Ty` (abstract payloads, never read);
//   * `Num(i64)` -> `Num(int)`;
//   * `fold_arith_ast(x)` on a `Box` field reads through the box (`*x`);
//   * `.clone()` dropped (the spec matches by value);
//   * the early `return` of the Bin/Un arms becomes the arm tail
//     (`if c { E } else { TAIL }`), since spec code has no `return`.
// Everything else is verbatim. A source shape outside these rewrites is
// FATAL in the extractor rather than emitted half-translated.
//
// The hand half below (payload types, the op-table model, the theorems) is
// the template `fold_arith_ast_pure.in`; it contains no copy of the
// implementation.

// HAND HALF of the fold_arith_ast pure module. The committed artifact is
// `fold_arith_ast_pure.rs`, which harness/verus-sync-foldarith.sh
// regenerates from `sh2perl/src/ir.rs`; this template is the part that is
// not generated (the payload types, the op-table model, and the theorems
// about the recursion). Do not paste the implementation here: it is
// emitted from the source on every regeneration, so there is no second
// copy of it to mistranscribe.
use vstd::prelude::*;

verus! {

// ---- hand: payload types the transcript needs -------------------------
// `fold_arith_ast` never inspects a `String` or an `IrType` — it clones
// them through — so the model keeps them abstract. Nothing else in this
// file reads their fields except the pinned constructors below, which
// only use `id` as an interned operator tag.
pub struct Str {
    pub id: int,
}
pub struct Ty {
    pub id: int,
}

// ---- hand: the op-table MODEL ----------------------------------------
// The shipped `fold_bin_op`/`fold_un_op` tables live in src/ir.rs and are
// proved in optimize_pass05.rs (value and refusal arms) under the same
// gate. This is the small model the recursion theorem is stated over:
// ids 1,2,4 are `+ - *`; 5 is `/` with bash's zero-divisor abort; 3 is
// unary minus. A shape outside the model refuses (None), which is exactly
// how the shipped table treats the arms it does not fold.
pub open spec fn fold_bin_op(op: Str, a: int, b: int) -> Option<int> {
    if op.id == 1 {
        Some(a + b)
    } else if op.id == 2 {
        Some(a - b)
    } else if op.id == 4 {
        Some(a * b)
    } else if op.id == 5 {
        if b == 0 {
            None
        } else {
            Some(a / b)
        }
    } else {
        None
    }
}

pub open spec fn fold_un_op(op: Str, a: int) -> Option<int> {
    if op.id == 3 {
        Some(0 - a)
    } else {
        None
    }
}

// ---- GENERATED: enum Ast + fold_arith_ast (from src/ir.rs) --------
pub enum Ast {
    Num(int),
    Var(Str),
    Ident(Str),
    Index {
        var: Str,
        key: Box<Ast>,
    },
    Bin {
        op: Str,
        lhs: Box<Ast>,
        rhs: Box<Ast>,
    },
    Un {
        op: Str,
        arg: Box<Ast>,
    },
    Cond {
        test: Box<Ast>,
        then: Box<Ast>,
        else_: Box<Ast>,
    },
    Assign {
        var: Str,
        op: Str,
        rhs: Box<Ast>,
    },
    IncDec {
        var: Str,
        delta: i64,
        prefix: bool,
    },
    Sizeof(Ty),
    Cast {
        ty: Ty,
        arg: Box<Ast>,
    },
}

pub open spec fn fold_arith_ast(ast: Ast) -> Ast
    decreases ast,
{
    match ast {
        Ast::Bin { op, lhs, rhs } => {
            let l = fold_arith_ast(*lhs);
            let r = fold_arith_ast(*rhs);
            // Fully-literal operands fold to a single `Num` so EVERY backend
            // emits the literal instead of re-deriving the arithmetic at
            // runtime (`3i64 * 4i64` in Rust, a `__sh_div` polyfill for
            // `10/2`, `int(a/b)` guards in Perl). REFUSE > GUESS: any op we
            // cannot evaluate to bash's exact int64 answer (div-by-zero, an
            // out-of-range shift, an overflow the renderer models
            // differently) keeps its shape.
            if let (Ast::Num(a), Ast::Num(b)) = (&l, &r) {
                if let Some(v) = fold_bin_op(op, *a, *b) {
                    Ast::Num(v)
                } else { Ast::Bin {
                op: op,
                lhs: Box::new(l),
                rhs: Box::new(r),
            } }
            } else { Ast::Bin {
                op: op,
                lhs: Box::new(l),
                rhs: Box::new(r),
            } }}
        Ast::Un { op, arg } => {
            let a = fold_arith_ast(*arg);
            // A fully-literal unary operand folds too (`$((-3))` → -3).
            if let Ast::Num(n) = &a {
                if let Some(v) = fold_un_op(op, *n) {
                    Ast::Num(v)
                } else { Ast::Un {
                op: op,
                arg: Box::new(a),
            } }
            } else { Ast::Un {
                op: op,
                arg: Box::new(a),
            } }}
        Ast::Cond { test, then, else_ } => Ast::Cond {
            test: Box::new(fold_arith_ast(*test)),
            then: Box::new(fold_arith_ast(*then)),
            else_: Box::new(fold_arith_ast(*else_)),
        },
        Ast::Index { var, key } => Ast::Index {
            var: var,
            key: Box::new(fold_arith_ast(*key)),
        },
        Ast::Assign { var, op, rhs } => Ast::Assign {
            var: var,
            op: op,
            rhs: Box::new(fold_arith_ast(*rhs)),
        },
        Ast::Cast { ty, arg } => Ast::Cast {
            ty: ty,
            arg: Box::new(fold_arith_ast(*arg)),
        },
        other => other,
    }
}

// ---- hand: theorems about the GENERATED recursion --------------------

// `no_literal_node`: no `Bin` has two literal operands and no `Un` has a
// literal operand, recursively. On such a tree the fold has nothing it is
// allowed to replace, so it must be the identity — the refusal property
// (REFUSE > GUESS) stated over the whole tree, not one node.
pub open spec fn no_literal_node(ast: Ast) -> bool
    decreases ast,
{
    match ast {
        Ast::Num(_) => true,
        Ast::Var(_) | Ast::Ident(_) | Ast::IncDec { .. } | Ast::Sizeof(_) => true,
        Ast::Bin { lhs, rhs, .. } => {
            !(matches!(*lhs, Ast::Num(_)) && matches!(*rhs, Ast::Num(_))) && no_literal_node(*lhs)
                && no_literal_node(*rhs)
        },
        Ast::Un { arg, .. } => !matches!(*arg, Ast::Num(_)) && no_literal_node(*arg),
        Ast::Cond { test, then, else_ } => {
            no_literal_node(*test) && no_literal_node(*then) && no_literal_node(*else_)
        },
        Ast::Index { key, .. } => no_literal_node(*key),
        Ast::Assign { rhs, .. } => no_literal_node(*rhs),
        Ast::Cast { arg, .. } => no_literal_node(*arg),
    }
}

proof fn fold_identity(ast: Ast)
    requires no_literal_node(ast),
    ensures fold_arith_ast(ast) == ast,
    decreases ast,
{
    match ast {
        Ast::Bin { lhs, rhs, .. } => {
            fold_identity(*lhs);
            fold_identity(*rhs);
        },
        Ast::Un { arg, .. } => {
            fold_identity(*arg);
        },
        Ast::Cond { test, then, else_ } => {
            fold_identity(*test);
            fold_identity(*then);
            fold_identity(*else_);
        },
        Ast::Index { key, .. } => {
            fold_identity(*key);
        },
        Ast::Assign { rhs, .. } => {
            fold_identity(*rhs);
        },
        Ast::Cast { arg, .. } => {
            fold_identity(*arg);
        },
        _ => {},
    }
}

// The value of a literal-only tree over the op model. A refused node is
// None — bash aborts there, and the fold must keep the shape, not invent
// a value (that is the `fold_value` case below).
pub open spec fn eval(ast: Ast) -> Option<int>
    decreases ast,
{
    match ast {
        Ast::Num(n) => Some(n),
        Ast::Bin { op, lhs, rhs } => match (eval(*lhs), eval(*rhs)) {
            (Some(a), Some(b)) => fold_bin_op(op, a, b),
            _ => None,
        },
        Ast::Un { op, arg } => match eval(*arg) {
            Some(a) => fold_un_op(op, a),
            None => None,
        },
        _ => None,
    }
}

pub open spec fn literal_ast(ast: Ast) -> bool
    decreases ast,
{
    match ast {
        Ast::Num(_) => true,
        Ast::Bin { lhs, rhs, .. } => literal_ast(*lhs) && literal_ast(*rhs),
        Ast::Un { arg, .. } => literal_ast(*arg),
        _ => false,
    }
}

// THE VALUE OBLIGATION: folding a literal tree does not change its value
// (a refused node is None on both sides — the fold keeps the shape).
proof fn fold_value(ast: Ast)
    requires literal_ast(ast),
    ensures eval(fold_arith_ast(ast)) == eval(ast),
    decreases ast,
{
    match ast {
        Ast::Bin { op, lhs, rhs } => {
            fold_value(*lhs);
            fold_value(*rhs);
            let l = fold_arith_ast(*lhs);
            let r = fold_arith_ast(*rhs);
            assert(eval(l) == eval(*lhs));
            assert(eval(r) == eval(*rhs));
            if let (Ast::Num(a), Ast::Num(b)) = (&l, &r) {
                assert(eval(l) == Some(*a));
                assert(eval(r) == Some(*b));
            }
        },
        Ast::Un { op, arg } => {
            fold_value(*arg);
            let a = fold_arith_ast(*arg);
            assert(eval(a) == eval(*arg));
        },
        _ => {},
    }
}

// ---- hand: fidelity pins (the transform's own test shapes) ------------

pub open spec fn plus() -> Str {
    Str { id: 1 }
}
pub open spec fn times() -> Str {
    Str { id: 4 }
}
pub open spec fn div() -> Str {
    Str { id: 5 }
}
pub open spec fn var_x() -> Str {
    Str { id: 9 }
}

// `3 * 4` folds to a single literal (the test table's fully-literal case).
proof fn pin_three_plus_four()
    ensures fold_arith_ast(Ast::Bin {
        op: plus(),
        lhs: Box::new(Ast::Num(3)),
        rhs: Box::new(Ast::Num(4)),
    }) == Ast::Num(7),
{
    reveal_with_fuel(fold_arith_ast, 8);
}

// `1 / 0` is a bash runtime error: the shape survives unchanged.
proof fn pin_div_zero_refuses()
    ensures fold_arith_ast(Ast::Bin {
        op: div(),
        lhs: Box::new(Ast::Num(1)),
        rhs: Box::new(Ast::Num(0)),
    }) == (Ast::Bin {
        op: div(),
        lhs: Box::new(Ast::Num(1)),
        rhs: Box::new(Ast::Num(0)),
    }),
{
    reveal_with_fuel(fold_arith_ast, 8);
}

// The recursion reaches children: an inner literal pair folds and the
// outer Bin keeps its shape (`$(( 2*3 + x ))` → `$(( 6 + x ))`).
proof fn pin_nested_literal_folds_and_shape_survives()
    ensures fold_arith_ast(Ast::Bin {
        op: plus(),
        lhs: Box::new(Ast::Bin {
            op: times(),
            lhs: Box::new(Ast::Num(2)),
            rhs: Box::new(Ast::Num(3)),
        }),
        rhs: Box::new(Ast::Var(var_x())),
    }) == (Ast::Bin {
        op: plus(),
        lhs: Box::new(Ast::Num(6)),
        rhs: Box::new(Ast::Var(var_x())),
    }),
{
    reveal_with_fuel(fold_arith_ast, 12);
}

fn main() {}

}
