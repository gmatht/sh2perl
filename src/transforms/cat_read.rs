//! cat / cat -n FILE → ForEachLine streaming loop.
//!
//! `cat FILE` is a per-line copy and `cat -n FILE` a numbered one — both
//! are O(1)-memory readline loops (docs shir-primitives.md §ForEachLine),
//! not whole-file slurps or fork/exec. Lowering them to the generic
//! ForEachLine node lets EVERY backend render them natively (perl
//! while(<$fh>), JS eachLine, C getline, Go bufio.Scanner, Zig reader,
//! py file-iterator) instead of spawning the external binary.
//!
//! Lifted shapes (conservative — anything else keeps the exec):
//!
//!   • `cat F1 [F2 …]`        — every operand a static string path
//!   • `cat -n F1 [F2 …]`     — bash/GNU numbering: "%6d\t%s" per line
//!
//! Numbered bodies use only GENERIC IR: an arith counter increment plus
//! an exec("printf", …) output (every backend lowers printf natively).
//! Multiple files chain their ForEachLine blocks sequentially, exactly
//! bash's concatenate-in-order semantics. STATEMENT-position only:
//! `v=$(cat -n F)` captures keep their exec (value-channel consumers).

use crate::ir::{ArithAst, IrExpr, IrStmt};
use crate::shir_nodes::ForEachLine;
use std::sync::atomic::{AtomicUsize, Ordering};

static SEQ: AtomicUsize = AtomicUsize::new(0);

pub fn transform(stmts: &mut Vec<IrStmt>) -> bool {
    let mut changed = false;
    for st in stmts.iter_mut() {
        changed |= lift_stmt(st);
    }
    changed
}

fn lift_stmt(st: &mut IrStmt) -> bool {
    // statement-position cat: replace the WHOLE statement (the Ext/Block
    // is itself a statement — expr-position mutation cannot express it)
    if let IrStmt::Expr(e) = st {
        if let IrExpr::Call { func, args } = e {
            if (func == "exec" || func == "builtin")
                && matches!(args.as_slice(), [IrExpr::Str(c, _), IrExpr::Array(_)] if c == "cat")
            {
                if let Some(repl) = build_cat_replacement(args) {
                    *st = repl;
                    return true;
                }
            }
        }
        return false;
    }
    match st {
        IrStmt::Block(b)
        | IrStmt::Function { body: b, .. }
        | IrStmt::While { body: b, .. }
        | IrStmt::For { body: b, .. } => transform(b),
        IrStmt::If { then: b, elsifs, else_, .. } => {
            let mut changed = transform(b);
            for (_, eb) in elsifs.iter_mut() {
                changed |= transform(eb);
            }
            changed |= transform(else_);
            changed
        }
        _ => false,
    }
}

fn build_cat_replacement(args: &[IrExpr]) -> Option<IrStmt> {
    let Some(IrExpr::Array(items)) = args.get(1) else {
        return None;
    };
    // parse: leading -n flag; every remaining operand a static path
    let mut numbered = false;
    let mut files: Vec<String> = Vec::new();
    for it in items.iter() {
        match it {
            IrExpr::Str(s, _) if s.starts_with('-') => {
                if s == "-n" && !numbered && files.is_empty() {
                    numbered = true;
                } else {
                    return None; // other flags / misplaced -n: keep exec
                }
            }
            IrExpr::Str(s, _) => files.push(s.clone()),
            _ => return None, // dynamic source: keep exec
        }
    }
    if files.is_empty() {
        return None; // bare `cat` reads stdin: keep exec
    }

    let seq = SEQ.fetch_add(1, Ordering::Relaxed) + 1;
    let line_var = format!("__cl{seq}");
    let num_var = format!("__cn{seq}");
    let mut blocks: Vec<IrStmt> = Vec::new();
    for file in &files {
        let body = if numbered {
            vec![
                counter_incr(&num_var),
                printf_line(&line_var, &num_var),
            ]
        } else {
            vec![output_line(&line_var)]
        };
        blocks.push(IrStmt::Ext(Box::new(ForEachLine {
            source: IrExpr::Str(file.clone(), crate::ir::StrStyle::DoubleQuoted),
            var: line_var.clone(),
            limit: None,
            body,
        })));
    }
    Some(if blocks.len() == 1 {
        blocks.pop().unwrap()
    } else {
        IrStmt::Block(blocks)
    })
}

fn counter_incr(name: &str) -> IrStmt {
    IrStmt::Assign {
        targets: vec![crate::ir::AssignTarget {
            var: name.to_string(),
            sigil: None,
            indices: Vec::new(),
        }],
        expr: IrExpr::Arith(Box::new(ArithAst::Bin {
            op: "+".to_string(),
            lhs: Box::new(ArithAst::Var(name.to_string())),
            rhs: Box::new(ArithAst::Num(1)),
        })),
        asm: None,
    }
}

fn printf_line(line_var: &str, num_var: &str) -> IrStmt {
    IrStmt::Expr(IrExpr::Call {
        func: "exec".to_string(),
        args: vec![
            IrExpr::Str("printf".to_string(), crate::ir::StrStyle::DoubleQuoted),
            IrExpr::Array(vec![
                // GNU cat -n: "%6d\t%s\n"
                IrExpr::Str(
                    "%6d\\t%s\\n".to_string(),
                    crate::ir::StrStyle::DoubleQuoted,
                ),
                IrExpr::Var(num_var.to_string(), None),
                IrExpr::Var(line_var.to_string(), None),
            ]),
        ],
    })
}

fn output_line(line_var: &str) -> IrStmt {
    IrStmt::Output {
        value: IrExpr::Var(line_var.to_string(), None),
        newline: true,
        target: None,
    }
}
