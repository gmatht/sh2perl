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


FAMILIES = {
    "f1": (RUST_HELPERS, emit_pin),
    "status_reads": (STATUS_HELPERS, emit_status_pin),
    "uae": (UAE_HELPERS, emit_uae_pin),
    "grep": (GREP_HELPERS, emit_grep_pin),
    "pipeline": (PIPELINE_HELPERS, emit_pipeline_pin),
    "command_v": (COMMAND_V_HELPERS, emit_command_v_pin),
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
