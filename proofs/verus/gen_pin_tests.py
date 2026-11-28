#!/usr/bin/env python3
"""Generate `cargo test --lib` tests from a Verus manifest's `[pins]` rows.

Why this exists: a manifest row `pin P -> UNMIRRORED (no test)` records a
concrete witness that nothing executes. Such rows rot silently — the proof
can drift, the fold can regress, and the table still reads the same. This
script closes that loop for a pin family declared in a JSON spec:

    python3 proofs/verus/gen_pin_tests.py --emit   # write the Rust test module
    python3 proofs/verus/gen_pin_tests.py --run    # emit, run, and flip rows
    python3 proofs/verus/gen_pin_tests.py --check  # run only; never write

A pin is flipped to `MAPPED` (and its `exec` row to `: PASS`) ONLY when the
generated test actually passed under `cargo test --lib`; a MAPPED row whose
test is red is demoted back to UNMIRRORED. Nothing here guesses: if the spec
names a pin the manifest does not list, or the manifest lists a spec pin
twice, it fails instead of writing.

The generated tests call the REAL functions the proofs model, through a child
module of their own file, so a test cannot pass against a re-implementation.
Each spec entry also carries bash's answer for the same witness (`bash`); where
it differs from `expect`, the pin records a DIVERGENCE, exactly as the
line-mode pins do.
"""

from __future__ import annotations

import json
import os
import re
import subprocess
import sys

HERE = os.path.dirname(os.path.abspath(__file__))
REPO = os.path.dirname(os.path.dirname(HERE))

RUST_HELPERS = '''use super::*;

fn stage_of(cmd: &str, words: &[&str]) -> IrExpr {
    IrExpr::Arrow(vec![IrStmt::Expr(IrExpr::Call {
        func: "exec".to_string(),
        args: vec![st(cmd), IrExpr::Array(words.iter().map(|w| st(w)).collect())],
    })])
}

fn pipeline(producer: &str, pwords: &[&str], consumer: &str, cwords: &[&str]) -> IrStmt {
    IrStmt::Expr(IrExpr::Call {
        func: "pipeline".to_string(),
        args: vec![IrExpr::Array(vec![
            stage_of(producer, pwords),
            stage_of(consumer, cwords),
        ])],
    })
}

/// The printed text of a folded statement (`Output` blocks only).
fn printed(st: &IrStmt) -> Option<String> {
    let IrStmt::Block(items) = st else { return None };
    match items.first() {
        Some(IrStmt::Output { value: IrExpr::Str(s, _), .. }) => Some(s.clone()),
        _ => None,
    }
}

/// Whether a folded `Output` appends its own newline.
fn printed_newline(st: &IrStmt) -> Option<bool> {
    let IrStmt::Block(items) = st else { return None };
    match items.first() {
        Some(IrStmt::Output { newline, .. }) => Some(*newline),
        _ => None,
    }
}

/// The Perl snippet a `RawText` fold emits (the tr/sort/head cluster).
fn raw_text(st: &IrStmt) -> Option<String> {
    match st {
        IrStmt::RawText(s) => Some(s.clone()),
        _ => None,
    }
}
'''

STATUS_HELPERS = '''use super::*;
use crate::ir::{AsmSpec, AssignTarget, IrCaseClause, IrProgram, IrStmt, StrStyle};

fn st(s: &str) -> IrExpr {
    IrExpr::Str(s.to_string(), StrStyle::DoubleQuoted)
}

fn program(stmts: Vec<IrStmt>) -> IrProgram {
    IrProgram {
        stmts,
        subs: vec![],
        imports: vec![],
        requires: vec![],
        var_types: vec![],
        var_const: vec![],
        var_lifetimes: vec![],
        var_nospace: vec![],
        stmt_lines: vec![],
        var_bash_env: vec![],
        var_lengths: vec![],
        var_storage: vec![],
    }
}

fn target(name: &str) -> AssignTarget {
    AssignTarget {
        var: name.to_string(),
        sigil: None,
        indices: vec![],
    }
}

// -- witness statements -------------------------------------------------

fn test_operand() -> IrStmt {
    IrStmt::Expr(IrExpr::Call {
        func: "test".to_string(),
        args: vec![st("-z \\"$?\\"")],
    })
}

fn getvar_q() -> IrStmt {
    IrStmt::Expr(IrExpr::Call {
        func: "getVar".to_string(),
        args: vec![st("?")],
    })
}

fn echo_hi() -> IrStmt {
    IrStmt::Output {
        value: st("hi"),
        newline: true,
        target: None,
    }
}

fn exit_bare() -> IrStmt {
    IrStmt::Exit(None)
}

fn exit_zero() -> IrStmt {
    IrStmt::Exit(Some(IrExpr::Int(0)))
}

fn assign_one() -> IrStmt {
    IrStmt::Assign {
        targets: vec![target("x")],
        expr: IrExpr::Int(1),
        asm: None,
    }
}

fn asm_assign() -> IrStmt {
    IrStmt::Assign {
        targets: vec![target("x")],
        expr: IrExpr::Int(1),
        asm: Some(AsmSpec {
            template: "myx".to_string(),
            volatile: false,
            outputs: vec![],
            inputs: vec![],
            clobbers: vec![],
        }),
    }
}

fn two_targets() -> IrStmt {
    IrStmt::Assign {
        targets: vec![target("x"), target("y")],
        expr: IrExpr::Int(1),
        asm: None,
    }
}

fn capture_assign() -> IrStmt {
    IrStmt::Assign {
        targets: vec![target("x")],
        expr: IrExpr::Call {
            func: "capture".to_string(),
            args: vec![],
        },
        asm: None,
    }
}

fn capture_body_read() -> IrStmt {
    IrStmt::Expr(IrExpr::Call {
        func: "capture".to_string(),
        args: vec![IrExpr::Arrow(vec![test_operand()])],
    })
}

fn grep_matches() -> IrStmt {
    IrStmt::Expr(IrExpr::Call {
        func: "grepMatches".to_string(),
        args: vec![st("x")],
    })
}

fn plain_call() -> IrStmt {
    IrStmt::Expr(IrExpr::Call {
        func: "exec".to_string(),
        args: vec![st("x")],
    })
}

fn if_store() -> IrStmt {
    IrStmt::If {
        cond: IrExpr::Int(1),
        then: vec![],
        elsifs: vec![],
        else_: vec![],
    }
}

fn while_store() -> IrStmt {
    IrStmt::While {
        cond: IrExpr::Int(1),
        body: vec![],
    }
}

fn for_store() -> IrStmt {
    IrStmt::For {
        var: "i".to_string(),
        iter: IrExpr::Array(vec![]),
        body: vec![],
    }
}

fn case_store() -> IrStmt {
    IrStmt::Case {
        discriminant: IrExpr::Int(1),
        clauses: vec![],
    }
}

fn redirect_read() -> IrStmt {
    IrStmt::Redirect {
        inner: vec![test_operand()],
        redirects: vec![],
    }
}

fn redirect_after_store() -> IrStmt {
    IrStmt::Redirect {
        inner: vec![assign_one(), test_operand()],
        redirects: vec![],
    }
}

fn case_clause_read() -> IrStmt {
    IrStmt::Case {
        discriminant: IrExpr::Int(1),
        clauses: vec![IrCaseClause {
            patterns: vec![],
            body: vec![test_operand()],
        }],
    }
}

fn fn_body_read() -> IrStmt {
    IrStmt::Function {
        name: "f".to_string(),
        params: vec![],
        body: vec![test_operand()],
        named_blocks: vec![],
    }
}

/// The `local ec=$?` shape: two stores then a read, so the body localises
/// its own read and does NOT arm the enclosing list.
fn fn_localising_body() -> IrStmt {
    IrStmt::Function {
        name: "f".to_string(),
        params: vec![],
        body: vec![assign_one(), assign_one(), test_operand()],
        named_blocks: vec![],
    }
}
'''

UAE_HELPERS = '''use super::*;
use crate::ir::{BinOpKind, IrCaseClause, IrExpr, IrStmt, StrStyle};

fn str_(s: &str) -> IrExpr {
    IrExpr::Str(s.to_string(), StrStyle::DoubleQuoted)
}

fn echo(s: &str) -> IrStmt {
    IrStmt::Expr(IrExpr::Call {
        func: "builtin".to_string(),
        args: vec![str_("echo"), IrExpr::Array(vec![str_(s)])],
    })
}

fn while_true(body: Vec<IrStmt>) -> IrStmt {
    IrStmt::While {
        cond: IrExpr::Bool(true),
        body,
    }
}

fn case_break() -> IrStmt {
    IrStmt::Case {
        discriminant: str_("x"),
        clauses: vec![IrCaseClause {
            patterns: vec!["a".to_string()],
            body: vec![IrStmt::Break],
        }],
    }
}

/// `break || exit 1`: break as a CONTROL CALL, not `IrStmt::Break`.
fn break_or() -> IrStmt {
    IrStmt::Expr(IrExpr::BinOp {
        lhs: Box::new(IrExpr::Call {
            func: "break".to_string(),
            args: vec![],
        }),
        op: BinOpKind::Or,
        rhs: Box::new(str_("x")),
    })
}

fn if_false_exit() -> IrStmt {
    IrStmt::If {
        cond: IrExpr::Bool(false),
        then: vec![IrStmt::Exit(None)],
        elsifs: vec![],
        else_: vec![],
    }
}

fn case_exit() -> IrStmt {
    IrStmt::Case {
        discriminant: str_("x"),
        clauses: vec![IrCaseClause {
            patterns: vec!["a".to_string()],
            body: vec![IrStmt::Exit(None)],
        }],
    }
}

fn fn_with(body: Vec<IrStmt>) -> IrStmt {
    IrStmt::Function {
        name: "f".to_string(),
        params: vec![],
        body,
        named_blocks: vec![],
    }
}
'''

BUILDERS_STORE = {
    "s_s_r": "vec![assign_one(), assign_one(), test_operand()]",
    "r_s_s": "vec![test_operand(), assign_one(), assign_one()]",
    "r_s_r": "vec![test_operand(), assign_one(), test_operand()]",
    "s_s": "vec![assign_one(), assign_one()]",
    "s_exit": "vec![assign_one(), exit_bare()]",
    "s_nested_s": "vec![assign_one(), redirect_read(), assign_one()]",
    "r_s": "vec![test_operand(), assign_one()]",
    "empty": "vec![]",
    "r": "vec![test_operand()]",
}

BUILDERS = {
    "loop_tail": 'vec![while_true(vec![echo("tick")]), echo("dead")]',
    "loop_with_break": 'vec![while_true(vec![echo("tick"), IrStmt::Break]), echo("live")]',
    "loop_with_case_break": 'vec![while_true(vec![case_break()]), echo("live")]',
    "loop_with_nested_loop": 'vec![while_true(vec![while_true(vec![IrStmt::Break])]), echo("dead")]',
    "loop_with_break_call": 'vec![while_true(vec![break_or()]), echo("live")]',
    "loop_body_break": 'vec![while_true(vec![IrStmt::Break, echo("dead")])]',
    "block_continue": 'vec![IrStmt::Block(vec![echo("a"), IrStmt::Continue, echo("dead")])]',
    "if_exit_then_tail": 'vec![if_false_exit(), echo("live")]',
    "case_exit_then_tail": 'vec![case_exit(), echo("live")]',
    "return_then_echo": 'vec![IrStmt::Return(None), echo("live")]',
    "fn_return_then_echo": 'vec![fn_with(vec![IrStmt::Return(None), echo("dead")])]',
}

WRAP = {
    "program_reads": "program_reads_status(&program(vec![{w}()]))",
    "stmt_reads": "stmt_reads_status(&{w}())",
    "stmt_stores": "stmt_definitely_stores_status(&{w}())",
    "expr_stores": "expr_definitely_stores_status(&{w}())",
    "nested": "stmt_nested_reads_before_store(&{w}())",
}

CALLS = {
    "width": "native_head_tail_stmt",
    "wc_l": "native_wc_l_pipeline",
    "sort": "native_sort_stmt",
    "sort_refuse": "native_sort_stmt",
    "tr": "native_echo_tr_sort_stmt",
}


def lit(s: str) -> str:
    """A Rust string literal; JSON escaping is a subset of Rust's for ASCII."""
    return json.dumps(s)


def producer_args(node: list[str]) -> tuple[str, list[str]]:
    assert len(node) >= 1, node
    return node[0], node[1:]


def emit_pin(pin: dict) -> list[str]:
    kind = pin["kind"]
    assert kind in CALLS, f"unknown kind {kind}"
    pcmd, pwords = producer_args(pin["producer"])
    ccmd, cwords = producer_args(pin["consumer"])
    call = CALLS[kind]
    pw = "&[" + ", ".join(lit(w) for w in pwords) + "]"
    cw = "&[" + ", ".join(lit(w) for w in cwords) + "]"

    out = [
        "",
        f"/// Manifest row: `pin {pin['pin']}` in shir_native_f1_sync.txt.",
        f"/// bash prints {lit(pin['bash'])}; the fold prints "
        + (lit(pin["expect"]) if "expect" in pin else "(see the assertions below)") + ".",
        f"/// {pin['note']}",
        "#[test]",
        f"fn {pin['pin']}() {{",
        f"    let mut st = pipeline({lit(pcmd)}, {pw}, {lit(ccmd)}, {cw});",
    ]
    if kind == "sort_refuse":
        out += [
            f"    assert!(!{call}(&mut st),",
            '        "an unknown sort flag must keep the shell-out");',
            '    assert!(printed(&st).is_none(), "a refused shape is left untouched");',
        ]
    else:
        out += [
            f"    assert!({call}(&mut st),",
            '        "the fold declined the literal pipeline shape");',
        ]
        if kind == "tr":
            out += [
                '    let code = raw_text(&st).expect("the tr fold emits RawText");',
            ]
            for frag in pin.get("expect_contains", []):
                out.append(
                    f"    assert!(code.contains({lit(frag)}), "
                    '"the snippet must contain that fragment: {code}");'
                )
            for frag in pin.get("expect_absent", []):
                out.append(
                    f"    assert!(!code.contains({lit(frag)}), "
                    '"the snippet must not contain that fragment: {code}");'
                )
        else:
            out.append(
                f"    assert_eq!(printed(&st).as_deref(), Some({lit(pin['expect'])}));"
            )
            if "expect_newline" in pin:
                out.append(
                    f"    assert_eq!(printed_newline(&st), Some({str(pin['expect_newline']).lower()}));"
                )
    if "contrast" in pin:
        con = pin["contrast"]
        ccmd2, cwords2 = producer_args(con["consumer"])
        cw2 = "&[" + ", ".join(lit(w) for w in cwords2) + "]"
        out += [
            "    // the same input under the contrasting flag: this is the",
            "    // difference the pin names, measured on the same witness.",
            f"    let mut other = pipeline({lit(pcmd)}, {pw}, {lit(ccmd2)}, {cw2});",
            f"    assert!({call}(&mut other),",
            '        "the contrast shape is also a literal pipeline");',
            f"    assert_eq!(printed(&other).as_deref(), Some({lit(con['expect'])}));",
        ]
    out.append("}")
    return out


def emit_status_pin(pin: dict) -> list[str]:
    """One test for a `status_reads` pin: wrap a witness in the real predicate."""
    out = [
        "",
        "/// Manifest row: `pin %s` in status_reads_traversal_sync.txt." % pin["pin"],
        "/// " + pin["note"],
        "#[test]",
        "fn %s() {" % pin["pin"],
    ]
    kind = pin["kind"]
    if kind == "facts_capture":
        out += [
            "    let st = capture_assign();",
            '    assert!(!stmt_reads_status(&st), "a capture assignment reads no status");',
            '    assert!(!stmt_definitely_stores_status(&st), "and stores none");',
            '    assert!(!stmt_nested_reads_before_store(&st), "nor nests one");',
        ]
    elif kind == "liveness":
        for case in pin["cases"]:
            b = case["build"]
            assert b in BUILDERS_STORE, "unknown witness %s" % b
            out.append(
                "    let lv = status_store_liveness(&program(%s));" % BUILDERS_STORE[b]
            )
            if case.get("expect") is None:
                out.append(
                    '    assert!(lv.is_none(), "expected no localisation on %s");' % b
                )
            else:
                got = ", ".join("true" if x else "false" for x in case["expect"])
                out.append(
                    '    assert_eq!(lv, Some(vec![%s]), "liveness on %s");' % (got, b)
                )
    elif kind == "liveness_bodies":
        out += [
            "    let lv = status_liveness(&program(vec![fn_localising_body(), echo_hi(), echo_hi()]));",
            "    assert_eq!(lv.bodies.len(), 1);",
            "    assert_eq!(lv.bodies[0].1, Some(vec![false, true, true]),",
            '        "the body keeps only the store its own read observes");',
            "    assert_eq!(lv.main, Some(vec![false, false, true]),",
            '        "the enclosing list is not armed by the body read: only the exit store survives");',
        ]
    else:
        assert kind in WRAP, "unknown kind %s" % kind
        for case in pin["cases"]:
            expr = WRAP[kind].format(w=case["witness"])
            out.append(
                '    assert_eq!(%s, %s, "witness %s");'
                % (expr, "true" if case["expect"] else "false", case["witness"])
            )
    out.append("}")
    return out


def emit_uae_pin(pin: dict) -> list[str]:
    """One test for a `uae` pin: run the REAL transform on a built witness."""
    out = [
        "",
        "/// Manifest row: `pin %s` in uae_terminators_sync.txt." % pin["pin"],
        "/// " + pin["note"],
        "#[test]",
        "fn %s() {" % pin["pin"],
    ]
    for case in pin["cases"]:
        b = case["build"]
        assert b in BUILDERS, "unknown witness %s" % b
        out.append("    let mut stmts = %s;" % BUILDERS[b])
        out.append(
            '    assert_eq!(transform(&mut stmts), %s, "changed on %s");'
            % ("true" if case["changed"] else "false", b)
        )
        out.append(
            '    assert_eq!(stmts.len(), %d, "list length on %s");' % (case["len"], b)
        )
        if "body_len" in case:
            out += [
                "    match &stmts[0] {",
                '        IrStmt::While { body, .. } => assert_eq!(body.len(), %d, "loop body on %s"),'
                % (case["body_len"], b),
                '        other => panic!("expected the loop, got {other:?}"),',
                "    }",
            ]
        if "block_len" in case:
            out += [
                "    match &stmts[0] {",
                '        IrStmt::Block(b) => assert_eq!(b.len(), %d, "block list on %s"),'
                % (case["block_len"], b),
                '        other => panic!("expected the block, got {other:?}"),',
                "    }",
            ]
        if "fn_body_len" in case:
            out += [
                "    match &stmts[0] {",
                '        IrStmt::Function { body, .. } => assert_eq!(body.len(), %d, "function body on %s"),'
                % (case["fn_body_len"], b),
                '        other => panic!("expected the function, got {other:?}"),',
                "    }",
            ]
    out.append("}")
    return out


GREP_HELPERS = '''use super::*;

fn gw(words: &[&str]) -> Vec<IrExpr> {
    words.iter().map(|w| IrExpr::Str(w.to_string(), StrStyle::Raw)).collect()
}
'''


def emit_grep_pin(pin: dict) -> list[str]:
    kind = pin["kind"]
    out = [
        "",
        f"/// Manifest row: `pin {pin['pin']}` in grep_literal_fold_sync.txt.",
        f"/// {pin['note']}",
        "#[test]",
        f"fn {pin['pin']}() {{",
    ]
    if kind == "word":
        out += [
            f"    assert_eq!(word_boundary_hit({lit(pin['line'])}, {lit(pin['pat'])}), "
            + ("true" if pin["expect"] else "false") + ");",
        ]
    elif kind in ("grep", "grep_none"):
        words = "&[" + ", ".join(lit(w) for w in pin["words"]) + "]"
        got = f"grep_output(&gw({words}), {lit(pin['input'])})"
        if kind == "grep_none":
            out += [
                f"    assert!({got}.is_none(),",
                '        "the fold must refuse this shape and keep the shell-out");',
            ]
        else:
            out += [
                f"    assert_eq!({got}.as_deref(), Some({lit(pin['expect'])}));",
            ]
    else:
        raise SystemExit(f"unknown grep kind {kind}")
    out.append("}")
    return out



PIPELINE_HELPERS = '''use super::*;

/// One pipeline stage as the IR the fold sees: an Arrow body holding a single
/// exec Call whose words are `words`.
fn cp_stage(cmd: &str, words: &[&str]) -> IrExpr {
    IrExpr::Arrow(vec![IrStmt::Expr(IrExpr::Call {
        func: "exec".to_string(),
        args: vec![st(cmd), IrExpr::Array(words.iter().map(|w| st(w)).collect())],
    })])
}

/// `pipeline [stage ...]` as a statement.
fn cp_pipeline(stages: Vec<IrExpr>) -> IrStmt {
    IrStmt::Expr(IrExpr::Call {
        func: "pipeline".to_string(),
        args: vec![IrExpr::Array(stages)],
    })
}

fn cp_stages(spec: &[(&str, &[&str])]) -> Vec<IrExpr> {
    spec.iter().map(|(c, w)| cp_stage(c, w)).collect()
}

/// The status the folded statement writes, when the fold fired.
fn cp_status(st: &IrStmt) -> Option<bool> {
    let IrStmt::Expr(IrExpr::Call { func, args }) = st else { return None };
    if func != "exec" { return None }
    let [IrExpr::Str(c, _), IrExpr::Array(w)] = args.as_slice() else { return None };
    if !w.is_empty() { return None }
    match c.as_str() {
        "true" => Some(true),
        "false" => Some(false),
        _ => None,
    }
}
'''


def emit_pipeline_pin(pin: dict) -> list[str]:
    kind = pin["kind"]
    out = [
        "",
        f"/// Manifest row: `pin {pin['pin']}` in shir_native_constant_pipeline_sync.txt.",
        f"/// {pin['note']}",
        "#[test]",
        f"fn {pin['pin']}() {{",
    ]
    spec = ", ".join(
        f"({lit(c)}, &[{', '.join(lit(w) for w in ws)}])" for c, ws in pin.get("stages", [])
    )
    if kind == "pipeline":
        out += [
            f"    let mut st = cp_pipeline(cp_stages(&[{spec}]));",
            "    assert!(native_constant_pipeline(&mut st),",
            '        "the fold must fire on a constant pipeline");',
            "    assert_eq!(cp_status(&st), Some(%s));" % ("true" if pin["expect"] else "false"),
        ]
    elif kind == "pipeline_refused":
        out += [
            f"    let mut st = cp_pipeline(cp_stages(&[{spec}]));",
            "    assert!(!native_constant_pipeline(&mut st),",
            '        "the fold must refuse this pipeline and keep the shell-out");',
            "    assert!(cp_status(&st).is_none());",
        ]
    elif kind == "pipeline_refused_multi":
        out += [
            "    let two_statements = IrExpr::Arrow(vec![",
            '        IrStmt::Expr(IrExpr::Call { func: "exec".to_string(),',
            '            args: vec![st("true"), IrExpr::Array(vec![])] }),',
            '        IrStmt::Expr(IrExpr::Call { func: "exec".to_string(),',
            '            args: vec![st("true"), IrExpr::Array(vec![])] }),',
            "    ]);",
            "    let mut st = cp_pipeline(vec![two_statements]);",
            "    assert!(!native_constant_pipeline(&mut st));",
        ]
    elif kind == "pipeline_refused_call":
        out += [
            "    let redirected = IrExpr::Arrow(vec![IrStmt::Expr(IrExpr::Call {",
            '        func: "redirect".to_string(),',
            '        args: vec![cp_stage("true", &[]), IrExpr::Array(vec![])],',
            "    })]);",
            "    let mut st = cp_pipeline(vec![redirected]);",
            "    assert!(!native_constant_pipeline(&mut st));",
        ]
    elif kind == "pipeline_refused_shape":
        out += [
            "    // not a pipeline at all",
            '    let mut bare = IrStmt::Expr(IrExpr::Call { func: "exec".to_string(),',
            '        args: vec![st("true"), IrExpr::Array(vec![])] });',
            "    assert!(!native_constant_pipeline(&mut bare));",
            "    // a `pipeline` whose argument is not the stage array",
            '    let mut odd = IrStmt::Expr(IrExpr::Call { func: "pipeline".to_string(),',
            '        args: vec![st("true")] });',
            "    assert!(!native_constant_pipeline(&mut odd));",
        ]
    elif kind == "pipefail":
        out += [
            "    // bash's answer without pipefail, and with it",
            '    let plain = std::process::Command::new("bash")',
            '        .arg("-c").arg("false | true").status().expect("bash");',
            '    assert!(plain.success(), "bash without pipefail exits 0 for `false | true`");',
            '    let strict = std::process::Command::new("bash")',
            '        .arg("-c").arg("set -o pipefail; false | true").status().expect("bash");',
            "    assert!(!strict.success(),",
            '        "under pipefail the failing stage decides: `false | true` exits 1");',
            "    // the fold, which has no pipefail gate, writes the plain answer",
            '    let mut st = cp_pipeline(cp_stages(&[("false", &[]), ("true", &[])]));',
            "    assert!(native_constant_pipeline(&mut st));",
            "    assert_eq!(cp_status(&st), Some(true),",
            '        "d1: the fold keeps the no-pipefail answer and cannot see pipefail");',
        ]
    else:
        raise SystemExit(f"unknown pipeline kind {kind}")
    out.append("}")
    return out


COMMAND_V_HELPERS = '''use super::*;

/// One redirect-spec object: the single fd-1 spec both callers insist on.
fn cv_spec(fd: i64, mode: &str, target: &str) -> IrExpr {
    IrExpr::Object(vec![
        ("fd".to_string(), IrExpr::Int(fd)),
        ("mode".to_string(), IrExpr::Str(mode.to_string(), StrStyle::Raw)),
        ("target".to_string(), IrExpr::Str(target.to_string(), StrStyle::Raw)),
    ])
}

/// `exec command FLAG NAME` as the inner statement.
fn cv_call(flag: &str, name: &str) -> IrStmt {
    IrStmt::Expr(IrExpr::Call {
        func: "exec".to_string(),
        args: vec![
            st("command"),
            IrExpr::Array(vec![st(flag), st(name)]),
        ],
    })
}

/// `redirect(command FLAG NAME, [spec])` — the shape both callers unwrap.
fn cv_redirected(spec: IrExpr) -> Vec<IrExpr> {
    vec![
        IrExpr::Arrow(vec![cv_call("-v", "ls")]),
        IrExpr::Array(vec![spec]),
    ]
}

/// The statement the fold writes, or None if it did not fire.
fn cv_status(st: &IrStmt) -> Option<bool> {
    let IrStmt::Expr(IrExpr::Call { func, args }) = st else { return None };
    if func != "exec" { return None }
    let [IrExpr::Str(c, _), IrExpr::Array(w)] = args.as_slice() else { return None };
    if !w.is_empty() { return None }
    match c.as_str() {
        "true" => Some(true),
        "false" => Some(false),
        _ => None,
    }
}

/// `bash -c "command -v NAME"` succeeded?
fn cv_bash_resolves(name: &str) -> bool {
    std::process::Command::new("bash")
        .arg("-c")
        .arg(format!("command -v {}", name))
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status()
        .expect("bash")
        .success()
}
'''


def emit_command_v_pin(pin: dict) -> list[str]:
    kind = pin["kind"]
    out = [
        "",
        f"/// Manifest row: `pin {pin['pin']}` in shir_native_command_v_sync.txt.",
        f"/// {pin['note']}",
        "#[test]",
        f"fn {pin['pin']}() {{",
    ]
    if kind == "cv_stmt":
        sink = pin["sink"]
        out.append(f'    let mut stmt = {pin["stmt_expr"]};')
        if pin["expect"] is None:
            out += ["    assert!(!native_command_v(&mut stmt),",
                    '        "the fold must refuse this shape and keep the shell-out");',
                    "    assert!(cv_status(&stmt).is_none());"]
        else:
            out += ["    assert!(native_command_v(&mut stmt),",
                    '        "the fold must fire on a /dev/null-redirected command -v");',
                    f"    assert_eq!(cv_status(&stmt), Some({'true' if pin['expect'] else 'false'}));"]
    elif kind == "cv_cond":
        out.append(f'    let args = {pin["cond_expr"]};')
        if pin["expect"] is None:
            out += ["    assert_eq!(command_v_redirect_truth(&args), None,",
                    '        "a condition must only be decided behind > /dev/null");']
        else:
            out += [f"    assert_eq!(command_v_redirect_truth(&args), Some({'true' if pin['expect'] else 'false'}));"]
    elif kind == "cv_bare_lossy":
        out += [
            '    let mut bare = IrStmt::Expr(IrExpr::Call { func: "exec".to_string(),',
            '        args: vec![st("command"), IrExpr::Array(vec![st("-v"), st("ls")])] });',
            "    // the fold fires on the bare form too ...",
            "    assert!(native_command_v(&mut bare));",
            "    assert_eq!(cv_status(&bare), Some(true));",
            "    // ... and bash prints the resolved path there, which the fold drops",
            '    assert!(cv_bash_resolves("ls"), "bash resolves ls");',
            '    let out = std::process::Command::new("bash")',
            '        .arg("-c").arg("command -v ls").output().expect("bash");',
            "    assert!(!out.stdout.is_empty(),",
            '        "bash prints the path, so the bare rewrite loses observable output (d1)");',
        ]
    elif kind == "cv_path":
        names = ", ".join(lit(n) for n in pin["names"])
        out += [
            f"    let names = [{names}];",
            "    for n in names {",
            '        assert!(cv_bash_resolves(n), "a name the fold list names is not on PATH");',
            "    }",
        ]
    elif kind == "cv_absent":
        out += [
            '    assert!(!cv_bash_resolves("nonexistent"),',
            '        "the fold treats `nonexistent` as absent, so bash must too");',
        ]
    elif kind == "cv_claims":
        names = ", ".join(lit(n) for n in pin["names"])
        out += [
            f"    let names = [{names}];",
            "    for n in names {",
            "        let mut stmt = IrStmt::Expr(IrExpr::Call { func: \"exec\".to_string(),",
            "            args: vec![st(\"command\"), IrExpr::Array(vec![st(\"-v\"), st(n)])] });",
            "        assert!(native_command_v(&mut stmt),",
            '            "the fold claims every name in its own list: {}", n);',
            "        assert_eq!(cv_status(&stmt), Some(true),",
            '            "a listed name always folds to status 0: {}", n);',
            "    }",
        ]
    elif kind == "cv_divergence":
        names = ", ".join(lit(n) for n in pin["names"])
        out += [
            f"    let names = [{names}];",
            "    let mut divergent = 0;",
            "    for n in names {",
            "        let mut stmt = IrStmt::Expr(IrExpr::Call { func: \"exec\".to_string(),",
            "            args: vec![st(\"command\"), IrExpr::Array(vec![st(\"-v\"), st(n)])] });",
            "        assert!(native_command_v(&mut stmt));",
            "        assert_eq!(cv_status(&stmt), Some(true));",
            "        if !cv_bash_resolves(n) {",
            "            // d2: the fold reports 0 where bash exits 1, because the list",
            "            // asserts a program this machine does not have",
            "            divergent += 1;",
            '            eprintln!("d2 divergence: command -v {} exits 1, the fold says 0", n);',
            "        }",
            "    }",
            '    eprintln!("d2: {} of {} listed names are absent here", divergent, names.len());',
        ]
    elif kind == "cv_copies":
        names = ", ".join(lit(n) for n in pin["names"])
        out += [
            f"    let names = [{names}];",
            "    let mut checked = 0;",
            "    for n in names {",
            "        // the statement fold, on a fresh statement",
            "        let mut stmt = IrStmt::Expr(IrExpr::Call { func: \"exec\".to_string(),",
            "            args: vec![st(\"command\"), IrExpr::Array(vec![st(\"-v\"), st(n)])] });",
            "        let fired = native_command_v(&mut stmt);",
            "        let stmt_status = cv_status(&stmt);",
            "        // the same question in condition position, behind the same sink,",
            "        // on a freshly built statement (the fold mutates its argument)",
            "        let cond = command_v_redirect_truth(&[",
            "            IrExpr::Arrow(vec![cv_call(\"-v\", n)]),",
            "            IrExpr::Array(vec![cv_spec(1, \"w\", \"/dev/null\")]),",
            "        ]);",
            "        // the two copies of the name list must agree, name by name",
            "        assert_eq!(fired, cond.is_some(),",
            '            "the two folds disagree about whether `{}` is known", n);',
            "        if fired {",
            "            assert_eq!(stmt_status, cond);",
            "            checked += 1;",
            "            if checked >= 6 { break; }",
            "        }",
            "    }",
            '    assert!(checked > 0, "at least one listed name must be known in both folds");',
        ]
    else:
        raise SystemExit(f"unknown command_v kind {kind}")
    out.append("}")
    return out


COMMAND_NOOP_COND_HELPERS = '''use super::*;

/// One redirect spec: fd, mode "w", target string.
fn cnn_spec(fd: i64, target: &str) -> IrExpr {
    IrExpr::Object(vec![
        ("fd".to_string(), IrExpr::Int(fd)),
        ("mode".to_string(), IrExpr::Str("w".to_string(), StrStyle::Raw)),
        ("target".to_string(), IrExpr::Str(target.to_string(), StrStyle::Raw)),
    ])
}

/// `redirect(Arrow([inner]), specs)`, optionally wrapped in `!`.
fn cnn_cond(not_wrapped: bool, inner: IrStmt, specs: Vec<IrExpr>) -> IrExpr {
    let redirect = IrExpr::Call {
        func: "redirect".to_string(),
        args: vec![IrExpr::Arrow(vec![inner]), IrExpr::Array(specs)],
    };
    if not_wrapped {
        IrExpr::BinOp {
            op: BinOpKind::Not,
            lhs: Box::new(redirect),
            rhs: Box::new(IrExpr::Bool(false)),
        }
    } else {
        redirect
    }
}

/// The redirect body: `words[0] words[1..]` as a single builtin call.
fn cnn_inner(words: &[&str]) -> IrStmt {
    IrStmt::Expr(IrExpr::Call {
        func: "builtin".to_string(),
        args: vec![
            st(words[0]),
            IrExpr::Array(words[1..].iter().map(|w| st(w)).collect()),
        ],
    })
}

/// An `If` with an empty body — all the fold looks at.
fn cnn_stmt(cond: IrExpr) -> IrStmt {
    IrStmt::If { cond, then: vec![], elsifs: vec![], else_: vec![] }
}

/// The Bool the fold wrote into the condition, or None if it did not fire.
fn cnn_written(st: &IrStmt) -> Option<bool> {
    let IrStmt::If { cond, .. } = st else { return None };
    match cond {
        IrExpr::Bool(b) => Some(*b),
        _ => None,
    }
}

/// The shell's own exit status for `script`.
fn cnn_bash(script: &str) -> i32 {
    std::process::Command::new("bash")
        .arg("-c")
        .arg(script)
        // the child's output must not interleave with the harness' verdict line
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status()
        .expect("bash")
        .code()
        .unwrap_or(-1)
}

'''

def emit_command_noop_cond_pin(pin: dict) -> list[str]:
    kind = pin["kind"]
    out = [
        "",
        f"/// Manifest row: `pin {pin['pin']}` in shir_native_command_noop_cond_sync.txt.",
        f"/// {pin['note']}",
        "#[test]",
        f"fn {pin['pin']}() {{",
    ]
    words = pin.get("words") or []
    argv = ", ".join(lit(w) for w in words)
    rest = ", ".join("st(%s)" % lit(w) for w in words[1:])
    parts = ", ".join(
        "cnn_spec(%d, %s)" % (s[0], lit(s[1])) for s in pin.get("specs") or []
    )
    specs = f"vec![{parts}]"
    nw = "true" if pin.get("not_wrapped") else "false"
    if kind == "cnn_fold":
        out += [
            f"    let mut stmt = cnn_stmt(cnn_cond({nw}, cnn_inner(&[{argv}]), {specs}));",
            '    assert!(native_command_noop_cond(&mut stmt),',
            '        "the fold must fire on a no-argument `command` behind a redirect");',
            f"    assert_eq!(cnn_written(&stmt), Some({'true' if pin['expect'] else 'false'}));",
        ]
    elif kind == "cnn_refuse":
        out += [
            f"    let mut stmt = cnn_stmt(cnn_cond({nw}, cnn_inner(&[{argv}]), {specs}));",
            "    assert!(!native_command_noop_cond(&mut stmt),",
            '        "this shape must keep the shell-out");',
            "    assert!(cnn_written(&stmt).is_none(),",
            '        "a refused fold must leave the condition alone");',
        ]
    elif kind == "cnn_refuse_worded":
        out += [
            f"    let mut stmt = cnn_stmt(cnn_cond({nw}, cnn_inner(&[{argv}]), {specs}));",
            "    assert!(!native_command_noop_cond(&mut stmt),",
            '        "a worded `command` must keep the shell-out");',
            "    assert!(cnn_written(&stmt).is_none());",
            "    // the refusal is right: the answer depends on the WORD",
            "    assert_eq!(cnn_bash(\"! command -v ls\"), 1, \"ls resolves\");",
            '    assert_eq!(cnn_bash("! command -v no-such-cmd-sh2perl"), 0,',
            '        "an unknown name flips the condition, so no constant is decided");',
        ]
    elif kind == "cnn_refuse_noredirect":
        out += [
            "    let call = IrExpr::Call { func: \"builtin\".to_string(),",
            f"        args: vec![st({lit(words[0])}), IrExpr::Array(vec![{rest}])] }};",
            "    let cond = if " + nw + " {",
            "        IrExpr::BinOp { op: BinOpKind::Not, lhs: Box::new(call.clone()),",
            "            rhs: Box::new(IrExpr::Bool(false)) }",
            "    } else { call };",
            "    let mut stmt = cnn_stmt(cond);",
            "    assert!(!native_command_noop_cond(&mut stmt),",
            '        "without a `redirect` Call there is no sink and no decision");',
            "    assert!(cnn_written(&stmt).is_none());",
        ]
    elif kind == "cnn_block_refuse":
        out += [
            "    let inner = IrStmt::Block(vec![cnn_inner(&[\"command\"]), cnn_inner(&[\"command\"])]);",
            f"    let mut stmt = cnn_stmt(cnn_cond({nw}, inner, {specs}));",
            "    assert!(!native_command_noop_cond(&mut stmt),",
            '        "a redirect body of two statements is not one status");',
            "    assert!(cnn_written(&stmt).is_none());",
        ]
    elif kind == "cnn_d3_devnull":
        out += [
            f"    let mut stmt = cnn_stmt(cnn_cond({nw}, cnn_inner(&[{argv}]), {specs}));",
            "    assert!(native_command_noop_cond(&mut stmt));",
            f"    assert_eq!(cnn_written(&stmt), Some({'true' if pin['expect'] else 'false'}));",
            "    // d3a measured: the redirect succeeds and `command` exits 0,",
            "    // so bash's plain condition is TRUE where the fold wrote false",
            "    assert_eq!(cnn_bash(\"command > /dev/null\"), 0);",
        ]
    elif kind == "cnn_d3_dynamic":
        out += [
            "    let dir = std::env::temp_dir();",
            "    let inp = dir.join(format!(\"cnn_in_{}.txt\", std::process::id()));",
            "    let outp = dir.join(format!(\"cnn_out_{}.txt\", std::process::id()));",
            "    std::fs::write(&inp, \"x\").unwrap();",
            "    std::fs::write(&outp, \"\").unwrap();",
            "    let (i, o) = (inp.to_str().unwrap(), outp.to_str().unwrap());",
            f"    let mut stmt = cnn_stmt(cnn_cond({nw}, cnn_inner(&[{argv}]),",
            "        vec![cnn_spec(0, i), cnn_spec(1, o)]));",
            "    assert!(native_command_noop_cond(&mut stmt));",
            f"    assert_eq!(cnn_written(&stmt), Some({'true' if pin['expect'] else 'false'}));",
            "    // d3b measured: with openable files `! command <in >out` is FALSE,",
            "    // the opposite of the constant the fold wrote",
            "    assert_eq!(cnn_bash(&format!(\"! command <'{}' >'{}'\", i, o)), 1);",
            "    std::fs::remove_file(&inp).ok();",
            "    std::fs::remove_file(&outp).ok();",
        ]
    elif kind == "cnn_bash_empty":
        out += [
            "    // the law's premise: an empty redirect target can never be opened",
            "    assert_eq!(cnn_bash(\"command <'' >''\"), 1);",
            "    assert_eq!(cnn_bash(\"! command <'' >''\"), 0);",
            "    // and `command` itself succeeds when the redirect does not fail",
            "    assert_eq!(cnn_bash(\"command\"), 0);",
        ]
    else:
        raise SystemExit(f"unknown command_noop_cond kind {kind}")
    out.append("}")
    return out


AFFIX_HELPERS = '''use super::*;

fn afx_s(v: &str) -> IrExpr {
    IrExpr::Str(v.to_string(), StrStyle::DoubleQuoted)
}

/// `param(word, word, …)` — the parser's spelling of every affix site.
fn afx_call(words: &[&str]) -> IrExpr {
    IrExpr::Call {
        func: "param".to_string(),
        args: words.iter().map(|w| afx_s(w)).collect(),
    }
}

/// Run `f` with the fold's target set, the way the pass is driven.
fn afx_target<T>(t: &str, f: impl FnOnce() -> T) -> T {
    crate::transforms::set_target(Some(t));
    let r = f();
    crate::transforms::set_target(None);
    r
}

/// A canonical spelling of what the pass produced, so the manifest's expected
/// node can be written out in full.
fn afx_shape(e: &IrExpr) -> String {
    match e {
        IrExpr::Ext(n) => {
            let j = n.to_json();
            match n.tag() {
                "AffixStrip" => format!(
                    "affix(prefix={}, pattern={})",
                    j["prefix"].as_bool().unwrap_or(false),
                    j["pattern"]["value"].as_str().unwrap_or("<nonliteral>")
                ),
                "CutAt" => format!(
                    "cut(head={}, first={}, delim={})",
                    j["head"].as_bool().unwrap_or(false),
                    j["first"].as_bool().unwrap_or(false),
                    j["delim"].as_str().unwrap_or("<nonliteral>")
                ),
                other => format!("node({other})"),
            }
        }
        IrExpr::Str(s, _) => format!("str({s})"),
        IrExpr::Call { func, args } if func == "param" && args.len() == 2 => match (&args[0], &args[1])
        {
            (IrExpr::Str(op, _), IrExpr::Str(name, _)) if op.is_empty() => format!("plain({name})"),
            _ => "param2(?)".to_string(),
        },
        IrExpr::Call { func, .. } => format!("call({func})"),
        _ => "other".to_string(),
    }
}

/// The folded node's value on `value`, using Rust's own strip — the node's
/// contract, not the transform's code path.
fn afx_value_of(e: &IrExpr, value: &str) -> Option<String> {
    let IrExpr::Ext(n) = e else { return None };
    if n.tag() != "AffixStrip" {
        return None;
    }
    let j = n.to_json();
    let pat = j["pattern"]["value"].as_str()?;
    let prefix = j["prefix"].as_bool()?;
    Some(if prefix {
        value.strip_prefix(pat).unwrap_or(value).to_string()
    } else {
        value.strip_suffix(pat).unwrap_or(value).to_string()
    })
}

/// What BASH prints for `${v<op><lit>}` on `value` — the independent side.
fn afx_bash(value: &str, op: &str, lit: &str) -> String {
    let script = format!("v=\\"{value}\\"; printf %s \\"${{v{op}{lit}}}\\"");
    let out = std::process::Command::new("bash")
        .arg("-c")
        .arg(&script)
        .stderr(std::process::Stdio::null())
        .output()
        .expect("bash");
    String::from_utf8_lossy(&out.stdout).to_string()
}

/// `echo ARG` where ARG is the affix site — the nesting the visit must reach.
fn afx_nested() -> IrStmt {
    IrStmt::Expr(IrExpr::Call {
        func: "builtin".to_string(),
        args: vec![afx_s("echo"), IrExpr::Array(vec![afx_call(&["#", "v", "lit"])])],
    })
}
'''


def emit_affix_pin(pin: dict) -> list[str]:
    kind = pin["kind"]
    target = pin.get("target", "rs")
    out = [
        "",
        f"/// Manifest row: `pin {pin['pin']}` in affix_strip_fold_sync.txt.",
        f"/// {pin['note']}",
        "#[test]",
        f"fn {pin['pin']}() {{",
        f'    afx_target({lit(target)}, || {{',
    ]
    if kind == "affix_node":
        for case in pin["cases"]:
            words = case.get("argv") or [case["op"], case["name"], case["pat"]]
            argv = ", ".join(lit(w) for w in words)
            exp = case["expect"]
            out += [f"        let mut e = afx_call(&[{argv}]);", "        let fired = expr(&mut e);"]
            if exp is None:
                out += [
                    "        assert!(!fired,",
                    f'            "must keep {words} untouched");',
                    '        assert_eq!(afx_shape(&e), "call(param)");',
                ]
            else:
                out += [
                    "        assert!(fired,",
                    f'            "must fold {words}");',
                    f"        assert_eq!(afx_shape(&e), {lit(exp)});",
                ]
    elif kind == "affix_value":
        for case in pin["cases"]:
            argv = ", ".join(lit(w) for w in (case["op"], "v", case["lit"]))
            out += [
                f"        let mut e = afx_call(&[{argv}]);",
                f'        assert!(expr(&mut e), "must fold {case["op"]}{case["lit"]!r}");',
                f'        let got = afx_value_of(&e, {lit(case["value"])}).expect("AffixStrip");',
                f"        assert_eq!(got, {lit(case['expect'])});",
                f"        assert_eq!(afx_bash({lit(case['value'])}, {lit(case['op'])}, {lit(case['lit'])}), got,",
                '            "bash must print what the node evaluates to");',
            ]
    elif kind == "affix_pair":
        for case in pin["cases"]:
            ops = case["ops"]
            a, b = ops[0], ops[1]
            out += [
                f'        let mut short = afx_call(&[{lit(a)}, "v", {lit(case["lit"])}]);',
                f'        let mut long = afx_call(&[{lit(b)}, "v", {lit(case["lit"])}]);',
                "        assert!(expr(&mut short) && expr(&mut long));",
                "        assert_eq!(afx_shape(&short), afx_shape(&long),",
                f'            "{a} and {b} must be one strip for a literal");',
                f'        let sv = afx_value_of(&short, {lit(case["value"])}).expect("AffixStrip");',
                "        assert_eq!(sv, afx_value_of(&long, {0}).unwrap());".format(lit(case["value"])),
                f"        assert_eq!(sv, {lit(case['expect'])});",
                f"        assert_eq!(afx_bash({lit(case['value'])}, {lit(a)}, {lit(case['lit'])}), sv);",
                f"        assert_eq!(afx_bash({lit(case['value'])}, {lit(b)}, {lit(case['lit'])}), sv,",
                '            "the longest form must print the same as the shortest");',
            ]
    elif kind == "affix_star":
        for case in pin["cases"]:
            prefix = "true" if case["op"] == "#" else "false"
            out += [
                f'        let mut e = afx_call(&[{lit(case["op"])}, "v", {lit(case["pat"])}]);',
                f'        assert!(expr(&mut e), "the star shape must fold");',
                f'        assert_eq!(afx_shape(&e), "affix(prefix={prefix}, pattern={case["body"]})");',
                f'        let got = afx_value_of(&e, {lit(case["value"])}).expect("AffixStrip");',
                f"        assert_eq!(afx_bash({lit(case['value'])}, {lit(case['op'])}, {lit(case['pat'])}), got,",
                '            "the star form prints what the node prints");',
                f"        assert_eq!(afx_bash({lit(case['value'])}, {lit(case['literal_op'])}, {lit(case['body'])}), got,",
                '            "and that is the literal form the star collapses to");',
            ]
    elif kind == "affix_double_star":
        d = pin["delim"]
        v = pin["value"]
        out += [
            f'        let mut first = afx_call(&["#", "v", {lit("*" + d)}]);',
            f'        let mut last = afx_call(&["##", "v", {lit("*" + d)}]);',
            "        assert!(expr(&mut first) && expr(&mut last));",
            f'        assert_eq!(afx_shape(&first), "cut(head=false, first=true, delim={d})");',
            f'        assert_eq!(afx_shape(&last), "cut(head=false, first=false, delim={d})");',
            "        assert_ne!(afx_shape(&first), afx_shape(&last),",
            '            "#*X and ##*X are different nodes");',
            f"        assert_eq!(afx_bash({lit(v)}, \"#\", {lit('*' + d)}), \"bX\",",
            '            "bash keeps the tail after the FIRST match");',
            f"        assert_eq!(afx_bash({lit(v)}, \"##\", {lit('*' + d)}), \"\",",
            '            "and drops everything through the LAST match");',
        ]
    elif kind == "affix_nested":
        out += [
            "        let mut st = afx_nested();",
            "        assert!(stmt(&mut st), \"the visit must descend into echo args\");",
            "        let IrStmt::Expr(IrExpr::Call { args, .. }) = &st else { panic!(\"shape\") };",
            "        let IrExpr::Array(items) = &args[1] else { panic!(\"array\") };",
            '        assert_eq!(afx_shape(&items[0]), "affix(prefix=true, pattern=lit)");',
        ]
    else:
        raise SystemExit(f"unknown affix kind {kind}")
    out += ["    });", "}"]
    return out


FAMILIES = {
    "f1": (RUST_HELPERS, emit_pin),
    "status_reads": (STATUS_HELPERS, emit_status_pin),
    "uae": (UAE_HELPERS, emit_uae_pin),
    "affix": (AFFIX_HELPERS, emit_affix_pin),
    "grep": (GREP_HELPERS, emit_grep_pin),
    "pipeline": (PIPELINE_HELPERS, emit_pipeline_pin),
    "command_v": (COMMAND_V_HELPERS, emit_command_v_pin),
    "command_noop_cond": (COMMAND_NOOP_COND_HELPERS, emit_command_noop_cond_pin),
}


def render(spec: dict) -> str:
    helpers, emit = FAMILIES[spec.get("family", "f1")]
    out = ["// " + line for line in spec["header"]]
    out.append("")
    out.extend(helpers.rstrip("\n").splitlines())
    for pin in spec["pins"]:
        out.extend(emit(pin))
    return "\n".join(out) + "\n"


def ensure_registration(spec: dict) -> bool:
    path = os.path.join(REPO, spec["register_in"])
    text = open(path, encoding="utf-8").read()
    if 'path = "%s"' % os.path.basename(spec["rust_file"]) in text:
        return False
    with open(path, "a", encoding="utf-8") as fh:
        if not text.endswith("\n"):
            fh.write("\n")
        fh.write(spec["register_block"])
    return True


def manifest_path(spec: dict) -> str:
    return os.path.join(REPO, spec["manifest_dir"], spec["manifest"] + "_sync.txt")


def check_rows(spec: dict, text: str) -> list[str]:
    """Every spec pin must be a real manifest row; no duplicates either way."""
    listed = re.findall(r"^pin\s+(\w+)", text, re.M)
    problems = []
    for pin in spec["pins"]:
        if listed.count(pin["pin"]) != 1:
            problems.append(
                f"spec pin {pin['pin']} appears {listed.count(pin['pin'])} time(s) "
                f"in the manifest (want exactly 1)"
            )
    return problems


def run_tests(names: list[str]) -> dict[str, bool]:
    cmd = ["cargo", "test", "--lib", "pin_tests", "--", "--test-threads", "1"]
    print(f"$ {' '.join(cmd)}   (in {REPO})", flush=True)
    proc = subprocess.run(cmd, cwd=REPO, capture_output=True, text=True, timeout=1800)
    out = proc.stdout + proc.stderr
    verdicts: dict[str, bool] = {}
    for full, verdict in re.findall(r"test\s+(\S+)\s+\.\.\.\s+(ok|FAILED)", out):
        base = full.split("::")[-1]
        if base in names:
            verdicts[base] = verdicts.get(base, True) and verdict == "ok"
    if proc.returncode != 0 and not verdicts:
        print(out[-3000:], file=sys.stderr)
    return verdicts


def flip(spec: dict, text: str, passed: set[str]) -> tuple[str, list[str]]:
    """Flip pin rows to MAPPED and make sure every passed pin HAS a PASS row.

    A manifest may list only some pins in [exec] (shir_native_f1 lists four of
    eleven), so a flipped pin with no exec row would be `MAPPED but unrun` to
    check_sync — the exact contradiction this script exists to prevent. Missing
    PASS rows are therefore appended at the end of the [exec] section.
    """
    changed = []
    lines = text.splitlines()
    have_exec: set[str] = set()
    for i, line in enumerate(lines):
        # Two row spellings are in use: `pin X -> UNMIRRORED (...)` (F1) and
        # `pin X: UNMIRRORED (...)` (the status_reads manifests).
        m = re.match(r"^pin\s+(\w+)\s*:?\s*(?:->\s*)?UNMIRRORED", line)
        if m and m.group(1) in passed:
            lines[i] = f"pin {m.group(1)} -> test {m.group(1)}: MAPPED"
            changed.append(m.group(1))
            continue
        # The other direction: a MAPPED row whose test no longer passes must
        # NOT keep claiming execution. Demote it so the manifest cannot read as
        # evidence for a test that is red (or missing).
        m = re.match(r"^pin\s+(\w+)\s*(?:->\s*)?test\s+\w+\s*:\s*MAPPED", line)
        if m and m.group(1) not in passed:
            lines[i] = (
                f"pin {m.group(1)} -> UNMIRRORED (its generated test is red: "
                f"see proofs/verus/gen_pin_tests.py)"
            )
            changed.append(m.group(1))
            continue
        # A MAPPED row that names a hand-written test is re-pointed at the
        # generated one: the evidence must be the test this script ran.
        if m and m.group(1) in passed:
            want = f"pin {m.group(1)} -> test {m.group(1)}: MAPPED"
            if line != want:
                lines[i] = want
                changed.append(m.group(1))
            continue
        m = re.match(r"^exec\s+(\w+)", line)
        if not m:
            continue
        pin = m.group(1)
        have_exec.add(pin)
        if pin in passed:
            lines[i] = f"exec {pin} ({pin}): PASS"
        elif "MAPPED" in line or "PASS" in line:
            lines[i] = f"exec {pin}: UNMIRRORED (its generated test is red)"
    missing = [p for p in passed if p not in have_exec]
    if missing:
        if "[exec]" not in lines:
            lines += ["", "[exec]"]
        # Append inside [exec]: after the last exec line, else after the header.
        last = max(
            (i for i, l in enumerate(lines) if l.startswith("exec ")),
            default=lines.index("[exec]"),
        )
        for off, pin in enumerate(sorted(missing), start=1):
            lines.insert(last + off, f"exec {pin} ({pin}): PASS")
            changed.append(pin)
    return "\n".join(lines) + "\n", changed


def main() -> int:
    spec_path = sys.argv[1] if len(sys.argv) > 1 and not sys.argv[1].startswith("--") else None
    flags = [a for a in sys.argv[1:] if a.startswith("--")]
    if not spec_path:
        spec_path = os.path.join(HERE, "pin_specs", "f1_head_tail.json")
    spec = json.load(open(spec_path, encoding="utf-8"))
    mpath = manifest_path(spec)
    text = open(mpath, encoding="utf-8").read()

    problems = check_rows(spec, text)
    if problems:
        for p in problems:
            print(f"FAIL {p}", file=sys.stderr)
        return 1

    if "--emit" in flags or "--run" in flags:
        rust = os.path.join(REPO, spec["rust_file"])
        open(rust, "w", encoding="utf-8").write(render(spec))
        print(f"wrote {os.path.relpath(rust, REPO)} ({len(spec['pins'])} tests)")
        if ensure_registration(spec):
            print(f"registered the test module at the end of {spec['register_in']}")

    if not ("--run" in flags or "--check" in flags):
        return 0

    verdicts = run_tests([p["pin"] for p in spec["pins"]])
    passed = {n for n, ok in verdicts.items() if ok}
    for pin in spec["pins"]:
        state = "PASS" if pin["pin"] in passed else "FAIL/MISSING"
        print(f"  {pin['pin']}: {state}")

    if "--check" in flags:
        return 0 if len(passed) == len(spec["pins"]) else 1

    new_text, changed = flip(spec, text, passed)
    if changed:
        open(mpath, "w", encoding="utf-8").write(new_text)
        print(f"updated {len(changed)} row(s) in {os.path.relpath(mpath, REPO)}")
    if len(passed) != len(spec["pins"]):
        print("some tests did not pass; those rows stay UNMIRRORED", file=sys.stderr)
        return 1
    return 0


if __name__ == "__main__":
    sys.exit(main())
