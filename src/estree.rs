//! ESTree JSON emitter (v0).
//!
//! Lowers the shell AST directly to **standard ESTree JSON**, with shell
//! semantics expressed as calls into a documented `sh2.*` runtime namespace
//! (see PLAN.md §1.2). v0 covers a subset; any construct not yet lowered emits
//! a call to `sh2.unsupported(...)` so output is ALWAYS valid, deterministic
//! ESTree — the structural gate flags `sh2.unsupported` and the corpus gate
//! counts it as a not-implemented failure until lowered.
//!
//! v0 lowers from the raw shell AST (stable surface). When the ShIR
//! generalization lands (PLAN.md §3), this module reroutes to consume ShIR;
//! the ESTree node model below stays.
//!
//! Constraints (enforced at the structural gate, PLAN.md §2.2):
//! - standard ESTree node types only (never custom node types);
//! - async-only codegen (top-level `await`; no `*Sync` callees);
//! - every callee is in the `sh2.*` whitelist.

use crate::ast::*;
use serde::Serialize;
use std::collections::BTreeMap;

// ── ESTree node model (standard subset) ─────────────────────────────

#[derive(Debug, Clone, Serialize)]
pub struct Program {
    #[serde(rename = "type")]
    pub type_: &'static str,
    #[serde(rename = "sourceType")]
    pub source_type: &'static str,
    pub body: Vec<Stmt>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(tag = "type")]
pub enum Stmt {
    ExpressionStatement {
        expression: Expr,
    },
    BlockStatement {
        body: Vec<Stmt>,
    },
    IfStatement {
        test: Expr,
        consequent: Box<Stmt>,
        alternate: Option<Box<Stmt>>,
    },
    // Python-style try/except/else/finally (core request py-sh-go
    // 20260813): the guarded block + catch clause(s) + optional
    // finalizer. Standard ESTree: handler is a single CatchClause
    // (multi-arm chains are an if/else-if ladder INSIDE it — see the
    // lowering in shir.rs stmt_to_estree); the else suite lowers to a
    // post-try guarded block (Python else runs only when the try body
    // completed WITHOUT raising, and else-body exceptions must NOT be
    // caught by this statement's arms).
    TryStatement {
        block: Box<Stmt>,
        handler: Option<CatchClause>,
        finalizer: Option<Box<Stmt>>,
    },
    ThrowStatement {
        argument: Expr,
    },
    SwitchStatement {
        discriminant: Expr,
        cases: Vec<SwitchCase>,
    },
    WhileStatement {
        test: Expr,
        body: Box<Stmt>,
    },
    /// The native numeric-range loop — `for (let i = lo; i <= hi; i++)`
    /// — the `seq_range_for` transform's target (the hand-js ideal for
    /// `for i in $(seq lo hi)`). init is a `VariableDeclaration`
    /// (`let i = lo`); test the `< =` bound; update `i++`.
    ForStatement {
        init: Box<Stmt>,
        test: Expr,
        update: Expr,
        body: Box<Stmt>,
    },
    ForOfStatement {
        left: Box<Stmt>,
        right: Expr,
        body: Box<Stmt>,
    },
    VariableDeclaration {
        declarations: Vec<VariableDeclarator>,
        kind: &'static str,
    },
    BreakStatement {
        label: Option<String>,
    },
    ContinueStatement {
        label: Option<String>,
    },
    ReturnStatement {
        argument: Option<Expr>,
    },
}

#[derive(Debug, Clone, Serialize)]
pub struct SwitchCase {
    #[serde(rename = "type")]
    pub type_: &'static str,
    pub test: Option<Expr>,
    pub consequent: Vec<Stmt>,
}

/// The single catch clause of a `TryStatement` (standard ESTree). The
/// exception binding is a fixed generated identifier; `as`-bound names
/// are written into the runtime store (sh2.setVar) so the handler's var
/// reads see them.
#[derive(Debug, Clone, Serialize)]
pub struct CatchClause {
    #[serde(rename = "type")]
    pub type_: &'static str,
    pub param: Option<Box<Expr>>,
    pub body: Box<Stmt>,
}

#[derive(Debug, Clone, Serialize)]
pub struct VariableDeclarator {
    #[serde(rename = "type")]
    pub type_: &'static str,
    pub id: Expr,
    pub init: Option<Expr>,
}

/// The `regex` property of a JS regex Literal (pattern + flags).
#[derive(Debug, Clone, Serialize)]
pub struct RegexLiteral {
    pub pattern: String,
    pub flags: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(tag = "type")]
pub enum Expr {
    Identifier {
        name: String,
    },
    Literal {
        value: serde_json::Value,
        raw: Option<String>,
        // A JS regex literal (`/\s+/`) — standard ESTree carries it as a
        // `regex` property on the Literal node (value: {}). Emitted only
        // by the native wc -w lowering (the runtime's word-count split).
        #[serde(skip_serializing_if = "Option::is_none")]
        regex: Option<RegexLiteral>,
    },
    TemplateLiteral {
        quasis: Vec<TemplateElement>,
        expressions: Vec<Expr>,
    },
    CallExpression {
        callee: Box<Expr>,
        arguments: Vec<Expr>,
        optional: bool,
    },
    MemberExpression {
        object: Box<Expr>,
        property: Box<Expr>,
        computed: bool,
        optional: bool,
    },
    AwaitExpression {
        argument: Box<Expr>,
    },
    ArrowFunctionExpression {
        params: Vec<Expr>,
        body: ArrowBody,
        expression: bool,
        // Always async: closure bodies may contain `await` (nested execs),
        // and the runtime awaits every closure it invokes.
        r#async: bool,
    },
    ObjectExpression {
        properties: Vec<Property>,
    },
    ArrayExpression {
        elements: Vec<Option<Expr>>,
    },
    // `[...l]` — the native cut-string lift's code-point split (the
    // runtime cut builtin's exact `[...line]` positions base).
    SpreadElement {
        argument: Box<Expr>,
    },
    LogicalExpression {
        operator: String,
        left: Box<Expr>,
        right: Box<Expr>,
    },
    BinaryExpression {
        operator: String,
        left: Box<Expr>,
        right: Box<Expr>,
    },
    AssignmentExpression {
        operator: String,
        left: Box<Expr>,
        right: Box<Expr>,
    },
    ConditionalExpression {
        test: Box<Expr>,
        consequent: Box<Expr>,
        alternate: Box<Expr>,
    },
    UnaryExpression {
        operator: String,
        argument: Box<Expr>,
        prefix: bool,
    },
    SequenceExpression {
        expressions: Vec<Expr>,
    },
    // `new Promise(r => setTimeout(() => r(true), ms))` — the native
    // sleep lowering (src/shir.rs try_native_sleep): the exec spawn
    // collapses to a plain async timer. The executor arrow resolves
    // `true` (the statement's value feeds the errexit guard, which needs
    // truthiness like every exec statement's value).
    NewExpression {
        callee: Box<Expr>,
        arguments: Vec<Expr>,
    },
}

/// Arrow function body: an expression (`x => expr`) or a block
/// (`() => { ... }`). Serializes to standard ESTree (`Expression` or
/// `BlockStatement`).
#[derive(Debug, Clone, Serialize)]
#[serde(untagged)]
pub enum ArrowBody {
    Expr(Box<Expr>),
    Block(Box<Stmt>),
}

#[derive(Debug, Clone, Serialize)]
pub struct Property {
    #[serde(rename = "type")]
    pub type_: &'static str,
    pub key: Expr,
    pub value: Expr,
    pub kind: &'static str,
    pub computed: bool,
    pub shorthand: bool,
}

#[derive(Debug, Clone, Serialize)]
pub struct TemplateElement {
    #[serde(rename = "type")]
    pub type_: &'static str,
    pub value: TemplateElementValue,
    pub tail: bool,
}

#[derive(Debug, Clone, Serialize)]
pub struct TemplateElementValue {
    pub raw: String,
    pub cooked: Option<String>,
}

// ── Entry: shell AST → ESTree via the ShIR (PLAN.md §3) ───────────
// The lowering itself lives in src/shir.rs (ast_to_ir + shir_to_estree);
// this module only owns the ESTree node model + the sh2.* helpers.

/// Lower a parsed shell program to an ESTree `Program` (via the ShIR).
///
/// Runs a pre-pass that rewrites process substitution (`<(...)`) into
/// constructs the ShIR can lower (here-string stdin + a materialized-path
/// argument); see `transform_process_substitution`.
pub fn ast_to_estree(commands: &[Command]) -> Program {
    let transformed: Vec<Command> = commands.iter().map(transform_cmd).collect();
    let ir = crate::shir::ast_to_ir(&transformed);
    fix_control_flow(crate::shir::shir_to_estree(&ir))
}

/// Convenience: lower + serialize (deterministic, compact JSON).
pub fn ast_to_estree_json(commands: &[Command]) -> Result<String, serde_json::Error> {
    let transformed: Vec<Command> = commands.iter().map(transform_cmd).collect();
    let ir = crate::shir::ast_to_ir(&transformed);
    serde_json::to_string(&fix_control_flow(crate::shir::shir_to_estree(&ir)))
}

// ── control-flow legality pass ───────────────────────────────────────
//
// The ShIR emits native `break`/`continue`/`return` statements, but in the
// generated JS all loops are `sh2.*Loop` calls whose bodies are ARROW
// functions, so a native `break`/`continue` inside a loop body (or a
// top-level `return`) is a SyntaxError. This pass rewrites those illegal
// statements back to `sh2.break()`/`sh2.continue()`/`sh2.return()` calls,
// which throw control Signals the runtime loop functions catch. Native
// `break` inside a `switch` (case clauses) and `return` inside a function
// arrow stay native (both legal).

fn fix_control_flow(prog: Program) -> Program {
    Program {
        type_: prog.type_,
        source_type: prog.source_type,
        body: prog
            .body
            .into_iter()
            .filter_map(|s| fix_stmt(s, false, false, false))
            .collect(),
    }
}

/// `in_arrow` — inside an ArrowFunctionExpression body; `in_func` — inside
/// the sh2.define function arrow (only there is a native `return` legal:
/// bash `return` inside a loop body / pipeline stage / subshell exits the
/// enclosing FUNCTION, which in generated JS is a loop-body arrow the
/// runtime must unwind via the RETURN signal); `in_switch` — directly
/// inside a switch-case consequent (native break is legal there).
// ── raw-byte preservation (non-UTF-8 sources) ───────────────────────
//
// The CLI decodes non-UTF-8 source bytes (>= 0x80) to U+F800+byte private-
// use chars before parsing (see cli/src/cli_commands.rs parse_file_to_estree).
// bash passes those bytes through unchanged, so the emitted strings map them
// to a `\x01SH2BYTE\x01<HEX>\x01` marker; the runtime's emit()/writeFileSync
// decode the marker back into the raw byte, so stdout matches bash
// byte-for-byte.
const RAW_BYTE_MAGIC: &str = "\u{1}SH2BYTE\u{1}";

fn map_raw_bytes(s: &str) -> String {
    if !s.chars().any(|c| (0xF800..=0xF8FF).contains(&(c as u32))) {
        return s.to_string();
    }
    let mut out = String::new();
    for c in s.chars() {
        let u = c as u32;
        if (0xF800..=0xF8FF).contains(&u) {
            out.push_str(RAW_BYTE_MAGIC);
            out.push_str(&format!("{:02X}", (u - 0xF800) as u8));
            out.push('\u{1}');
        } else {
            out.push(c);
        }
    }
    out
}

fn fix_stmt(stmt: Stmt, in_arrow: bool, in_func: bool, in_switch: bool) -> Option<Stmt> {
    Some(match stmt {
        Stmt::BreakStatement { label } if in_arrow && !in_switch => Stmt::ExpressionStatement {
            expression: sh2_call("break", vec![]),
        },
        Stmt::ContinueStatement { label } if in_arrow && !in_switch => Stmt::ExpressionStatement {
            expression: sh2_call("continue", vec![]),
        },
        Stmt::ReturnStatement { argument } if !in_arrow || !in_func => {
            let mut args = vec![];
            if let Some(a) = argument {
                args.push(a);
            }
            Stmt::ExpressionStatement {
                expression: sh2_call("return", args),
            }
        }
        Stmt::ExpressionStatement { expression } => Stmt::ExpressionStatement {
            expression: fix_expr(expression, in_arrow, in_func),
        },
        Stmt::BlockStatement { body } => Stmt::BlockStatement {
            body: body
                .into_iter()
                .filter_map(|s| fix_stmt(s, in_arrow, in_func, false))
                .collect(),
        },
        Stmt::IfStatement {
            test,
            consequent,
            alternate,
        } => Stmt::IfStatement {
            test: fix_expr(test, in_arrow, in_func),
            consequent: Box::new(
                fix_stmt(*consequent, in_arrow, in_func, false)
                    .unwrap_or(Stmt::BlockStatement { body: vec![] }),
            ),
            alternate: alternate.map(|a| {
                Box::new(
                    fix_stmt(*a, in_arrow, in_func, false)
                        .unwrap_or(Stmt::BlockStatement { body: vec![] }),
                )
            }),
        },
        // try/catch/finally: the guard block, handler and finalizer
        // share the enclosing context (a native break/continue/return
        // inside them is illegal or bash-return-position exactly as in
        // a plain block). The catch param is the fixed generated `e`
        // binding — never rewritten.
        Stmt::TryStatement {
            block,
            handler,
            finalizer,
        } => Stmt::TryStatement {
            block: Box::new(
                fix_stmt(*block, in_arrow, in_func, false)
                    .unwrap_or(Stmt::BlockStatement { body: vec![] }),
            ),
            handler: handler.map(|h| CatchClause {
                type_: h.type_,
                param: h.param,
                body: Box::new(
                    fix_stmt(*h.body, in_arrow, in_func, false)
                        .unwrap_or(Stmt::BlockStatement { body: vec![] }),
                ),
            }),
            finalizer: finalizer.map(|f| {
                Box::new(
                    fix_stmt(*f, in_arrow, in_func, false)
                        .unwrap_or(Stmt::BlockStatement { body: vec![] }),
                )
            }),
        },
        Stmt::ThrowStatement { argument } => Stmt::ThrowStatement {
            argument: fix_expr(argument, in_arrow, in_func),
        },
        Stmt::SwitchStatement {
            discriminant,
            cases,
        } => Stmt::SwitchStatement {
            discriminant: fix_expr(discriminant, in_arrow, in_func),
            cases: cases
                .into_iter()
                .map(|c| SwitchCase {
                    type_: c.type_,
                    test: c.test.map(|t| fix_expr(t, in_arrow, in_func)),
                    consequent: c
                        .consequent
                        .into_iter()
                        .filter_map(|s| fix_stmt(s, in_arrow, in_func, true))
                        .collect(),
                })
                .collect(),
        },
        Stmt::WhileStatement { test, body } => Stmt::WhileStatement {
            test: fix_expr(test, in_arrow, in_func),
            body: Box::new(
                fix_stmt(*body, in_arrow, in_func, false)
                    .unwrap_or(Stmt::BlockStatement { body: vec![] }),
            ),
        },
        Stmt::ForOfStatement { left, right, body } => Stmt::ForOfStatement {
            left,
            right: fix_expr(right, in_arrow, in_func),
            body: Box::new(
                fix_stmt(*body, in_arrow, in_func, false)
                    .unwrap_or(Stmt::BlockStatement { body: vec![] }),
            ),
        },
        Stmt::ForStatement {
            init,
            test,
            update,
            body,
        } => Stmt::ForStatement {
            init: Box::new(
                fix_stmt(*init, in_arrow, in_func, false)
                    .unwrap_or(Stmt::BlockStatement { body: vec![] }),
            ),
            test: fix_expr(test, in_arrow, in_func),
            update: fix_expr(update, in_arrow, in_func),
            body: Box::new(
                fix_stmt(*body, in_arrow, in_func, false)
                    .unwrap_or(Stmt::BlockStatement { body: vec![] }),
            ),
        },
        Stmt::VariableDeclaration { declarations, kind } => Stmt::VariableDeclaration {
            declarations: declarations
                .into_iter()
                .map(|d| VariableDeclarator {
                    type_: d.type_,
                    id: d.id,
                    init: d.init.map(|i| fix_expr(i, in_arrow, in_func)),
                })
                .collect(),
            kind,
        },
        other => other,
    })
}

fn fix_expr(e: Expr, in_arrow: bool, in_func: bool) -> Expr {
    match e {
        Expr::CallExpression {
            callee,
            arguments,
            optional,
        } => {
            // Arrows are bash-return contexts (loop bodies, pipeline stages,
            // subshells) EXCEPT the sh2.define function arrow, where a
            // native `return` is legal (and keeps the function's value).
            let is_define = matches!(
                callee.as_ref(),
                Expr::MemberExpression { object, property, .. }
                    if matches!(object.as_ref(), Expr::Identifier { name } if name == "sh2")
                        && matches!(property.as_ref(), Expr::Identifier { name } if name == "define")
            );
            Expr::CallExpression {
                callee: Box::new(fix_expr(*callee, in_arrow, in_func)),
                arguments: arguments
                    .into_iter()
                    .map(|a| fix_expr(a, in_arrow, if is_define { true } else { false }))
                    .collect(),
                optional,
            }
        }
        Expr::MemberExpression {
            object,
            property,
            computed,
            optional,
        } => Expr::MemberExpression {
            object: Box::new(fix_expr(*object, in_arrow, in_func)),
            property: Box::new(fix_expr(*property, in_arrow, in_func)),
            computed,
            optional,
        },
        Expr::AwaitExpression { argument } => Expr::AwaitExpression {
            argument: Box::new(fix_expr(*argument, in_arrow, in_func)),
        },
        Expr::ArrowFunctionExpression {
            params,
            body,
            expression,
            r#async,
        } => Expr::ArrowFunctionExpression {
            params,
            body: match body {
                ArrowBody::Expr(inner) => {
                    ArrowBody::Expr(Box::new(fix_expr(*inner, true, in_func)))
                }
                ArrowBody::Block(b) => ArrowBody::Block(Box::new(
                    fix_stmt(*b, true, in_func, false)
                        .unwrap_or(Stmt::BlockStatement { body: vec![] }),
                )),
            },
            expression,
            r#async,
        },
        Expr::ObjectExpression { properties } => Expr::ObjectExpression {
            properties: properties
                .into_iter()
                .map(|p| Property {
                    type_: p.type_,
                    key: p.key,
                    value: fix_expr(p.value, in_arrow, in_func),
                    kind: p.kind,
                    computed: p.computed,
                    shorthand: p.shorthand,
                })
                .collect(),
        },
        Expr::ArrayExpression { elements } => Expr::ArrayExpression {
            elements: elements
                .into_iter()
                .map(|el| el.map(|e| fix_expr(e, in_arrow, in_func)))
                .collect(),
        },
        Expr::Literal { value, raw, regex } => Expr::Literal {
            value: match value {
                serde_json::Value::String(s) => serde_json::Value::String(map_raw_bytes(&s)),
                other => other,
            },
            raw,
            regex,
        },
        Expr::TemplateLiteral {
            quasis,
            expressions,
        } => Expr::TemplateLiteral {
            quasis: quasis
                .into_iter()
                .map(|q| TemplateElement {
                    type_: q.type_,
                    value: TemplateElementValue {
                        raw: map_raw_bytes(&q.value.raw),
                        cooked: q.value.cooked.map(|c| map_raw_bytes(&c)),
                    },
                    tail: q.tail,
                })
                .collect(),
            expressions: expressions
                .into_iter()
                .map(|e| fix_expr(e, in_arrow, in_func))
                .collect(),
        },
        Expr::LogicalExpression {
            operator,
            left,
            right,
        } => Expr::LogicalExpression {
            operator,
            left: Box::new(fix_expr(*left, in_arrow, in_func)),
            right: Box::new(fix_expr(*right, in_arrow, in_func)),
        },
        Expr::UnaryExpression {
            operator,
            argument,
            prefix,
        } => Expr::UnaryExpression {
            operator,
            argument: Box::new(fix_expr(*argument, in_arrow, in_func)),
            prefix,
        },
        Expr::SequenceExpression { expressions } => Expr::SequenceExpression {
            expressions: expressions
                .into_iter()
                .map(|e| fix_expr(e, in_arrow, in_func))
                .collect(),
        },
        // the errexit-guard wrapper (`sh2._g = await sh2.forLoop(...)`)
        // puts loop calls inside an assignment — traverse it so loop-body
        // break/continue/return still get rewritten to sh2.* signals.
        Expr::AssignmentExpression {
            operator,
            left,
            right,
        } => Expr::AssignmentExpression {
            operator,
            left: Box::new(fix_expr(*left, in_arrow, in_func)),
            right: Box::new(fix_expr(*right, in_arrow, in_func)),
        },
        other => other,
    }
}

// ── lastExit-tail hoist ─────────────────────────────────────────────
//
// Lifts a constant `sh2.lastExit = N` write out of if/else common
// tails, then out of the enclosing loop:
//
//   if (c) { (write, sh2.lastExit = N) } else { sh2.lastExit = N }
//     →  if (c) { write } ; sh2.lastExit = N
//     →  (when that if is the loop body's tail)  sh2.lastExit = N after
//        the loop
//
// Phase 1 (if-hoist): an `if` whose consequent and alternate BOTH end
// with the same `sh2.lastExit = N` write (a standalone assignment, or
// the trailing element of a `(…, sh2.lastExit = N[, flag])` sequence)
// leaves `$?` = N on every path — the write moves after the if with
// identical semantics (the branches' own reads, all before the tail
// write, see the same values either way).
//
// Phase 2 (loop-hoist): a loop whose body's last statement is a
// standalone `sh2.lastExit = N` (the shape phase 1 leaves) can have it
// moved after the loop when the body contains NO other `sh2.lastExit`
// mention. Soundness:
//   - no body reads → the pre-loop value is never observed mid-loop
//     (every read point sees N before and after the move);
//   - the TRACKED native loop's body ends with the tracking READ
//     (`__sh2_loop_last = sh2.lastExit`), so phase 2's shape check only
//     fires on BARE loops — whose final status write is provably dead
//     (nothing observes `$?` after the loop before the next write), so
//     the 0-iteration difference (original leaves the pre-loop value,
//     hoisted writes N) is unobservable;
//   - the native numeric-range `for` and a materialized-list for-of are
//     ALWAYS bare but CAN run 0 times (`seq 5 1` / an empty range), so
//     they additionally require the loop provably runs ≥ 1 iteration.
//
// Runs post-emission on the Program, so every consumer (the CLI's
// --estree, the otranspilerl wasm, the corpus gate) sees it.
// Deterministic and idempotent (phase 2 leaves no trailing write to
// re-hoist).
pub(crate) fn hoist_last_exit(prog: Program) -> Program {
    Program {
        type_: prog.type_,
        source_type: prog.source_type,
        body: hoist_stmts(prog.body),
    }
}

/// `sh2.lastExit` member access?
fn is_last_exit_member(e: &Expr) -> bool {
    matches!(e, Expr::MemberExpression { object, property, computed: false, optional: false, .. }
        if matches!(&**object, Expr::Identifier { name } if name == "sh2")
            && matches!(&**property, Expr::Identifier { name } if name == "lastExit"))
}

/// `sh2.lastExit = <num literal>` → Some(N).
fn last_exit_assign_value(e: &Expr) -> Option<i64> {
    match e {
        Expr::AssignmentExpression { operator, left, right } if operator == "=" => {
            if !is_last_exit_member(left) {
                return None;
            }
            match &**right {
                Expr::Literal { value, .. } => value.as_i64(),
                _ => None,
            }
        }
        _ => None,
    }
}

fn is_pure_literal(e: &Expr) -> bool {
    matches!(e, Expr::Literal { .. })
}

/// A statement list's final statement must be an ExpressionStatement
/// whose expression ends with the `sh2.lastExit = N` write — standalone
/// assignment, `(…, sh2.lastExit = N)`, or `(…, sh2.lastExit = N, flag)`.
fn stmts_tail_write(stmts: &[Stmt]) -> Option<i64> {
    let last = stmts.last()?;
    let Stmt::ExpressionStatement { expression } = last else {
        return None;
    };
    match expression {
        Expr::AssignmentExpression { .. } => last_exit_assign_value(expression),
        Expr::SequenceExpression { expressions } => {
            let n = expressions.len();
            if n == 0 {
                return None;
            }
            if let Some(v) = last_exit_assign_value(&expressions[n - 1]) {
                return Some(v);
            }
            if n >= 2 && is_pure_literal(&expressions[n - 1]) {
                return last_exit_assign_value(&expressions[n - 2]);
            }
            None
        }
        _ => None,
    }
}

fn branch_tail_write(branch: &Stmt) -> Option<i64> {
    match branch {
        Stmt::BlockStatement { body } => stmts_tail_write(body),
        other => stmts_tail_write(std::slice::from_ref(other)),
    }
}

/// The common `sh2.lastExit = N` both branches end with (None when the
/// if has no else, or the branches' tail writes differ / are missing).
fn if_tail_write(stmt: &Stmt) -> Option<i64> {
    let Stmt::IfStatement { consequent, alternate, .. } = stmt else {
        return None;
    };
    let c = branch_tail_write(consequent)?;
    let a = branch_tail_write(alternate.as_deref()?)?;
    (c == a).then_some(c)
}

/// Remove the trailing `sh2.lastExit = n` write from a statement. None
/// when the statement was ONLY the write (drop it entirely).
fn strip_tail_write(stmt: Stmt, n: i64) -> Option<Stmt> {
    let Stmt::ExpressionStatement { expression } = stmt else {
        return Some(stmt);
    };
    match expression {
        Expr::AssignmentExpression { .. } => {
            if last_exit_assign_value(&expression) == Some(n) {
                None
            } else {
                Some(Stmt::ExpressionStatement { expression })
            }
        }
        Expr::SequenceExpression { mut expressions } => {
            let m = expressions.len();
            let write_at = if m >= 1 && last_exit_assign_value(&expressions[m - 1]) == Some(n) {
                Some(m - 1)
            } else if m >= 2
                && is_pure_literal(&expressions[m - 1])
                && last_exit_assign_value(&expressions[m - 2]) == Some(n)
            {
                Some(m - 2)
            } else {
                None
            };
            match write_at {
                Some(i) => {
                    expressions.remove(i);
                    match expressions.len() {
                        0 => None,
                        1 => Some(Stmt::ExpressionStatement {
                            expression: expressions.pop().unwrap(),
                        }),
                        _ => Some(Stmt::ExpressionStatement {
                            expression: Expr::SequenceExpression { expressions },
                        }),
                    }
                }
                None => Some(Stmt::ExpressionStatement {
                    expression: Expr::SequenceExpression { expressions },
                }),
            }
        }
        other => Some(Stmt::ExpressionStatement { expression: other }),
    }
}

/// Strip the trailing write from a branch; a branch that becomes empty
/// turns into an empty block.
fn strip_branch_tail(branch: &Stmt, n: i64) -> Stmt {
    match branch {
        Stmt::BlockStatement { body } => {
            let mut body = body.clone();
            if let Some(last) = body.last() {
                match strip_tail_write(last.clone(), n) {
                    Some(s) => *body.last_mut().unwrap() = s,
                    None => {
                        body.pop();
                    }
                }
            }
            Stmt::BlockStatement { body }
        }
        other => strip_tail_write(other.clone(), n)
            .unwrap_or_else(|| Stmt::BlockStatement { body: vec![] }),
    }
}

/// `sh2.lastExit = n;`
fn last_exit_write(n: i64) -> Stmt {
    Stmt::ExpressionStatement {
        expression: Expr::AssignmentExpression {
            operator: "=".to_string(),
            left: Box::new(sh2_member("lastExit")),
            right: Box::new(Expr::Literal {
                value: serde_json::Value::from(n),
                raw: None,
                regex: None,
            }),
        },
    }
}

/// Recurse into nested statement lists first (bottom-up), then run the
/// if-hoist and loop-hoist on this list.
fn hoist_stmts(stmts: Vec<Stmt>) -> Vec<Stmt> {
    let mut out: Vec<Stmt> = stmts.into_iter().map(hoist_stmt).collect();
    hoist_list(&mut out);
    out
}

fn hoist_stmt(stmt: Stmt) -> Stmt {
    match stmt {
        Stmt::BlockStatement { body } => Stmt::BlockStatement { body: hoist_stmts(body) },
        Stmt::IfStatement { test, consequent, alternate } => Stmt::IfStatement {
            test,
            consequent: Box::new(hoist_stmt(*consequent)),
            alternate: alternate.map(|a| Box::new(hoist_stmt(*a))),
        },
        Stmt::WhileStatement { test, body } => Stmt::WhileStatement {
            test,
            body: Box::new(hoist_stmt(*body)),
        },
        Stmt::TryStatement {
            block,
            handler,
            finalizer,
        } => Stmt::TryStatement {
            block: Box::new(hoist_stmt(*block)),
            handler: handler.map(|h| CatchClause {
                type_: h.type_,
                param: h.param,
                body: Box::new(hoist_stmt(*h.body)),
            }),
            finalizer: finalizer.map(|f| Box::new(hoist_stmt(*f))),
        },
        Stmt::ForStatement { init, test, update, body } => Stmt::ForStatement {
            init: Box::new(hoist_stmt(*init)),
            test,
            update,
            body: Box::new(hoist_stmt(*body)),
        },
        Stmt::ForOfStatement { left, right, body } => Stmt::ForOfStatement {
            left: Box::new(hoist_stmt(*left)),
            right,
            body: Box::new(hoist_stmt(*body)),
        },
        Stmt::SwitchStatement { discriminant, cases } => Stmt::SwitchStatement {
            discriminant,
            cases: cases
                .into_iter()
                .map(|c| SwitchCase {
                    type_: c.type_,
                    test: c.test,
                    consequent: hoist_stmts(c.consequent),
                })
                .collect(),
        },
        other => other,
    }
}

/// Phase 1 (if common tails) then phase 2 (loop tails) on one list.
fn hoist_list(stmts: &mut Vec<Stmt>) {
    // Phase 1 — if-hoists.
    let mut i = 0;
    while i < stmts.len() {
        if let Some(n) = if_tail_write(&stmts[i]) {
            let new_stmt = match stmts[i].clone() {
                Stmt::IfStatement { test, consequent, alternate } => {
                    let cons = strip_branch_tail(&consequent, n);
                    let alt = alternate.map(|a| strip_branch_tail(&a, n));
                    let alt = match alt {
                        Some(Stmt::BlockStatement { body }) if body.is_empty() => None,
                        other => other,
                    };
                    Stmt::IfStatement {
                        test,
                        consequent: Box::new(cons),
                        alternate: alt.map(Box::new),
                    }
                }
                _ => unreachable!("if_tail_write only matches IfStatement"),
            };
            stmts[i] = new_stmt;
            stmts.insert(i + 1, last_exit_write(n));
            i += 2;
        } else {
            i += 1;
        }
    }
    // Phase 2 — loop-hoists.
    let mut j = 0;
    while j < stmts.len() {
        match loop_tail_hoist(&stmts[j]) {
            Some((new_loop, n)) => {
                stmts[j] = new_loop;
                stmts.insert(j + 1, last_exit_write(n));
                j += 2;
            }
            None => j += 1,
        }
    }
}

/// The loop body's last statement as a standalone `sh2.lastExit = N`
/// (the shape phase 1 leaves).
fn body_tail_write(body: &Stmt) -> Option<i64> {
    match body {
        Stmt::BlockStatement { body } => stmts_tail_write(body),
        other => stmts_tail_write(std::slice::from_ref(other)),
    }
}

/// Phase 2: a loop whose body ends with a standalone `sh2.lastExit = N`
/// → hoist it after the loop (guards in the doc header).
fn loop_tail_hoist(stmt: &Stmt) -> Option<(Stmt, i64)> {
    let (body, n) = match stmt {
        Stmt::WhileStatement { body, .. } => (body, body_tail_write(body)?),
        Stmt::ForStatement { body, .. } => {
            let n = body_tail_write(body)?;
            if !for_provably_runs(stmt) {
                return None;
            }
            (body, n)
        }
        Stmt::ForOfStatement { body, right, .. } => {
            let n = body_tail_write(body)?;
            if !forof_provably_runs(right) {
                return None;
            }
            (body, n)
        }
        _ => return None,
    };
    // no OTHER sh2.lastExit mention anywhere in the body (reads or
    // mid-body writes would observe the pre-loop value)
    let stripped = strip_body_tail(body, n);
    if body_mentions_last_exit(&stripped) {
        return None;
    }
    let new_stmt = match stmt {
        Stmt::WhileStatement { test, .. } => Stmt::WhileStatement {
            test: test.clone(),
            body: Box::new(stripped),
        },
        Stmt::ForStatement { init, test, update, .. } => Stmt::ForStatement {
            init: init.clone(),
            test: test.clone(),
            update: update.clone(),
            body: Box::new(stripped),
        },
        Stmt::ForOfStatement { left, right, .. } => Stmt::ForOfStatement {
            left: left.clone(),
            right: right.clone(),
            body: Box::new(stripped),
        },
        _ => unreachable!(),
    };
    Some((new_stmt, n))
}

fn strip_body_tail(body: &Stmt, n: i64) -> Stmt {
    match body {
        Stmt::BlockStatement { body } => {
            let mut body = body.clone();
            if let Some(last) = body.last() {
                match strip_tail_write(last.clone(), n) {
                    Some(s) => *body.last_mut().unwrap() = s,
                    None => {
                        body.pop();
                    }
                }
            }
            Stmt::BlockStatement { body }
        }
        other => strip_tail_write(other.clone(), n)
            .unwrap_or_else(|| Stmt::BlockStatement { body: vec![] }),
    }
}

/// Any remaining `sh2.lastExit` mention in the (already-stripped) body?
/// Serialization-based: only the `sh2.lastExit` MemberExpression shape
/// serializes as `"name":"lastExit"` — a string literal containing
/// "lastExit" serializes as `"value":"lastExit"` and never matches.
fn body_mentions_last_exit(body: &Stmt) -> bool {
    serde_json::to_string(body)
        .map(|json| json.contains("\"name\":\"lastExit\""))
        .unwrap_or(true) // serialization failure → conservative veto
}

/// A native numeric-range `for (let i = lo; i <= hi; i++)` provably runs
/// at least once when the init satisfies the test. The update direction
/// is irrelevant — only the FIRST test matters.
fn for_provably_runs(stmt: &Stmt) -> bool {
    let Stmt::ForStatement { init, test, update, .. } = stmt else {
        return false;
    };
    let Stmt::VariableDeclaration { declarations, .. } = &**init else {
        return false;
    };
    let [d] = declarations.as_slice() else {
        return false;
    };
    let Expr::Identifier { name } = &d.id else {
        return false;
    };
    let Some(Expr::Literal { value, .. }) = &d.init else {
        return false;
    };
    let Some(lo) = value.as_i64() else {
        return false;
    };
    // the counter update shape: `i++` / `++i` / `i--` / `--i`
    match update {
        Expr::UnaryExpression { operator, argument, .. }
            if matches!(operator.as_str(), "++" | "--") =>
        {
            match &**argument {
                Expr::Identifier { name: n } if n == name => {}
                _ => return false,
            }
        }
        _ => return false,
    }
    let Expr::BinaryExpression { operator, left, right } = test else {
        return false;
    };
    let (x, lit) = match (&**left, &**right) {
        (Expr::Identifier { name: n }, Expr::Literal { value, .. }) if n == name => {
            let Some(lit) = value.as_i64() else { return false };
            (lo, lit)
        }
        (Expr::Literal { value, .. }, Expr::Identifier { name: n }) if n == name => {
            let Some(lit) = value.as_i64() else { return false };
            (lo, lit)
        }
        _ => return false,
    };
    cmp_value(operator, x, lit)
}

fn cmp_value(op: &str, a: i64, b: i64) -> bool {
    match op {
        "<" => a < b,
        "<=" => a <= b,
        ">" => a > b,
        ">=" => a >= b,
        "==" | "===" => a == b,
        "!=" | "!==" => a != b,
        _ => false,
    }
}

/// A for-of over a non-empty array literal provably runs ≥ 1 time (the
/// materialized range-item fallback — `seq 5 1` materializes to an
/// EMPTY array and stays unhoisted).
fn forof_provably_runs(right: &Expr) -> bool {
    match right {
        Expr::ArrayExpression { elements } => elements.iter().any(|e| e.is_some()),
        _ => false,
    }
}

// ── native-array lowering ───────────────────────────────────────────
//
// Migrated from the website's src/lower.js `lowerNativeArrays` (which
// the corpus gate could never see) into the emitter, so the gate holds
// it accountable. When the estree PROVES an array is simple — one
// `sh2.setArray("name", [..])` at top level, then only reads — the
// runtime store calls become dead weight:
//
//   sh2.setArray("arr", [a, b])            →  let arr = [a, b];
//   sh2.arrayIndex("arr", "1")             →  (arr[1] !== undefined ? arr[1] : "")
//   sh2.getVar("arr[1]")                   →  (arr[1] !== undefined ? arr[1] : "")
//   sh2.arrayLen("arr") / param slice "#arr" →  arr.length
//   sh2.arrayItems("arr") / param slice "arr" →  arr
//
// Conservative guards (stricter than lower.js, which predates the
// current emitter's arrayIndex/arrayItems shapes and never ran under
// the gate):
//   • exactly ONE setArray, and it must be a DIRECT top-level statement
//     (a nested/conditional setArray can't become a `let`);
//   • the setArray items are an ArrayExpression (a runtime-valued
//     initializer can't be a JS literal);
//   • every other ref is an element READ with a literal non-negative
//     integer index, a `len`, or a `join` — no whole-var reads (`$arr`),
//     no computed/arithmetic/negative subscripts (the runtime
//     evalArith's + wraps negatives), no `setVar`/`unset` writes;
//   • no ref inside a script-function arrow (`let __fn_* = … =>` / the
//     older `sh2.define` shape) — the function may run before the `let`
//     initializes (TDZ), and may shadow the name;
//   • no pre-existing declaration / bare identifier use of the name
//     anywhere (a native array's uses are all sh2 string args).

#[derive(Default)]
struct ArrayRefs {
    /// sh2.setArray("name", [..]) refs seen (any depth).
    set_arrays: usize,
    /// The program-body index of the top-level setArray statement.
    set_array_idx: Option<usize>,
    /// One of them is the DIRECT expression of a top-level statement.
    top_set_array: bool,
    /// Program-body indices of statements containing a READ of the name
    /// (element/len/join). A read must never EXECUTE before the setArray
    /// (a native `let` would hit the TDZ; the runtime returns "" for an
    /// as-yet-unset array).
    read_stmt_idxs: Vec<usize>,
    /// A whole-var read (`$arr` → getVar("name") / getVar("name[@]")).
    whole: bool,
    /// A write / unset / non-array-valued setArray / non-@ param.
    writes: bool,
    /// A ref inside a script-function arrow (deferred invocation).
    in_fn: bool,
    /// A computed / non-integer / negative-literal subscript read.
    index_bad: bool,
}

fn is_sh2_call(callee: &Expr, fn_name: &str) -> bool {
    matches!(callee, Expr::MemberExpression { object, property, computed: false, optional: false, .. }
        if matches!(&**object, Expr::Identifier { name } if name == "sh2")
            && matches!(&**property, Expr::Identifier { name: p } if p == fn_name))
}

fn sh2_callee_name(callee: &Expr) -> Option<&str> {
    match callee {
        Expr::MemberExpression { object, property, computed: false, optional: false, .. } => {
            match (&**object, &**property) {
                (Expr::Identifier { name: o }, Expr::Identifier { name: p }) if o == "sh2" => {
                    Some(p)
                }
                _ => None,
            }
        }
        _ => None,
    }
}

fn lit_str<'a>(e: &'a Expr) -> Option<&'a str> {
    match e {
        Expr::Literal { value, .. } => value.as_str(),
        _ => None,
    }
}

/// `"name[idx]"` → (name, idx) for a Literal getVar/setVar arg;
/// `[@]`/`[*]` and bare `"name"` are whole-var (None).
fn parse_var_arg_str(s: &str) -> Option<(&str, Option<&str>)> {
    let s = s.trim();
    if let Some(rest) = s.strip_suffix(']') {
        if let Some(open) = rest.rfind('[') {
            let (name, idx) = (&rest[..open], &rest[open + 1..]);
            if name.chars().next().map(|c| c.is_ascii_alphabetic() || c == '_').unwrap_or(false)
                && name.chars().all(|c| c.is_ascii_alphanumeric() || c == '_')
            {
                if idx == "@" || idx == "*" {
                    return Some((name, None)); // whole-array form
                }
                return Some((name, Some(idx)));
            }
        }
    }
    Some((s, None))
}

/// Classify one sh2 call into the per-name accumulator.
fn classify_array_call(
    fn_name: &str,
    args: &[Expr],
    in_fn: bool,
    stmt_idx: usize,
    top_set_arrays: &std::collections::HashSet<String>,
    acc: &mut std::collections::HashMap<String, ArrayRefs>,
) {
    let Some(first) = args.first() else { return };
    let name = match fn_name {
        "setArray" | "setArrayAppend" | "arrayLen" | "arrayItems" | "unset" => match lit_str(first) {
            Some(n) => n,
            None => return,
        },
        "getVar" | "setVar" => {
            // Literal "name[idx]" / "name", or template `name[${i}]`
            match first {
                Expr::Literal { value, .. } => {
                    let Some(s) = value.as_str() else { return };
                    match parse_var_arg_str(s) {
                        Some((n, _)) => n,
                        None => return,
                    }
                }
                Expr::TemplateLiteral { quasis, expressions } => {
                    if quasis.len() == 2 && expressions.len() == 1 {
                        let head = quasis[0].value.raw.trim_end_matches('[');
                        if quasis[1].value.raw.trim() == "]" {
                            head
                        } else {
                            return;
                        }
                    } else {
                        return;
                    }
                }
                _ => return,
            }
        }
        "arrayIndex" => match lit_str(first) {
            Some(n) => n,
            None => return,
        },
        "param" => {
            let Some(target_arg) = args.get(1) else { return };
            let Some(target) = lit_str(target_arg) else { return };
            let Some(op) = lit_str(first) else { return };
            // slice-of-a-name is an array ref; other params are not
            if op == "slice" {
                target.trim_start_matches('#')
            } else {
                return;
            }
        }
        _ => return,
    };
    let entry = acc.entry(name.to_string()).or_default();
    if in_fn {
        entry.in_fn = true;
    }
    match fn_name {
        "setArray" => {
            entry.set_arrays += 1;
            if top_set_arrays.contains(name) {
                entry.top_set_array = true;
                entry.set_array_idx = Some(stmt_idx);
            }
            if !matches!(args.get(1), Some(Expr::ArrayExpression { .. })) {
                entry.writes = true;
            }
        }
        "setArrayAppend" => entry.writes = true,
        "getVar" => match args.first() {
            Some(Expr::Literal { value, .. }) => {
                if let Some((_, Some(idx))) = parse_var_arg_str(value.as_str().unwrap_or("")) {
                    if idx.parse::<i64>().map(|v| v >= 0).unwrap_or(false) {
                        // plain literal element read — OK
                        entry.read_stmt_idxs.push(stmt_idx);
                    } else {
                        entry.index_bad = true;
                    }
                } else {
                    entry.whole = true; // bare name / [@] / [*]
                }
            }
            Some(Expr::TemplateLiteral { .. }) => entry.index_bad = true,
            _ => {}
        },
        "arrayIndex" => match args.get(1) {
            Some(Expr::Literal { value, .. }) => {
                if value
                    .as_i64()
                    .or_else(|| value.as_str().and_then(|s| s.parse::<i64>().ok()))
                    .map(|v| v >= 0)
                    .unwrap_or(false)
                {
                    // plain literal element read — OK
                    entry.read_stmt_idxs.push(stmt_idx);
                } else {
                    entry.index_bad = true;
                }
            }
            _ => entry.index_bad = true,
        },
        "arrayLen" | "arrayItems" => entry.read_stmt_idxs.push(stmt_idx),
        "param" => {
            let op = args.first().and_then(lit_str).unwrap_or("");
            let target = args.get(1).and_then(lit_str).unwrap_or("");
            let mode = args.get(2).and_then(lit_str).unwrap_or("");
            if op == "slice" && mode == "@" {
                // len (#name) or join (name) — both reads
                entry.read_stmt_idxs.push(stmt_idx);
            } else {
                entry.writes = true;
            }
        }
        "setVar" => entry.writes = true,
        "unset" => entry.writes = true,
        _ => {}
    }
}

/// Walk every expr in a statement; `in_fn` becomes true inside a
/// script-function arrow (`let __fn_* = … =>` / `sh2.define(…, … =>)`).
fn walk_stmt_exprs(stmt: &Stmt, in_fn: bool, f: &mut impl FnMut(&Expr, bool)) {
    match stmt {
        Stmt::ExpressionStatement { expression } => walk_expr(expression, in_fn, f),
        Stmt::BlockStatement { body } => {
            for s in body {
                walk_stmt_exprs(s, in_fn, f);
            }
        }
        Stmt::IfStatement { test, consequent, alternate, .. } => {
            walk_expr(test, in_fn, f);
            walk_stmt_exprs(consequent, in_fn, f);
            if let Some(a) = alternate {
                walk_stmt_exprs(a, in_fn, f);
            }
        }
        Stmt::SwitchStatement { discriminant, cases, .. } => {
            walk_expr(discriminant, in_fn, f);
            for c in cases {
                for s in &c.consequent {
                    walk_stmt_exprs(s, in_fn, f);
                }
            }
        }
        Stmt::WhileStatement { test, body, .. } => {
            walk_expr(test, in_fn, f);
            walk_stmt_exprs(body, in_fn, f);
        }
        Stmt::TryStatement {
            block,
            handler,
            finalizer,
            ..
        } => {
            walk_stmt_exprs(block, in_fn, f);
            if let Some(h) = handler {
                if let Some(p) = &h.param {
                    walk_expr(p, in_fn, f);
                }
                walk_stmt_exprs(&h.body, in_fn, f);
            }
            if let Some(fin) = finalizer {
                walk_stmt_exprs(fin, in_fn, f);
            }
        }
        Stmt::ForStatement { init, test, update, body, .. } => {
            walk_stmt_exprs(init, in_fn, f);
            walk_expr(test, in_fn, f);
            walk_expr(update, in_fn, f);
            walk_stmt_exprs(body, in_fn, f);
        }
        Stmt::ForOfStatement { left, right, body, .. } => {
            walk_stmt_exprs(left, in_fn, f);
            walk_expr(right, in_fn, f);
            walk_stmt_exprs(body, in_fn, f);
        }
        Stmt::VariableDeclaration { declarations, .. } => {
            for d in declarations {
                walk_expr(&d.id, in_fn, f);
                if let Some(init) = &d.init {
                    // `let __fn_<name> = async (...) => {…}` — a script
                    // function's body (deferred; scope-safety guard)
                    let fn_arrow = matches!(&d.id, Expr::Identifier { name } if name.starts_with("__fn_"))
                        && matches!(init, Expr::ArrowFunctionExpression { .. });
                    walk_expr(init, if fn_arrow { true } else { in_fn }, f);
                }
            }
        }
        Stmt::BreakStatement { .. } | Stmt::ContinueStatement { .. } => {}
        Stmt::ReturnStatement { argument } => {
            if let Some(a) = argument {
                walk_expr(a, in_fn, f);
            }
        }
        Stmt::ThrowStatement { argument } => walk_expr(argument, in_fn, f),
    }
}

fn walk_expr(e: &Expr, in_fn: bool, f: &mut impl FnMut(&Expr, bool)) {
    f(e, in_fn);
    match e {
        Expr::Identifier { .. } | Expr::Literal { .. } => {}
        Expr::TemplateLiteral { expressions, .. } => {
            for q in expressions {
                walk_expr(q, in_fn, f);
            }
        }
        Expr::CallExpression { callee, arguments, .. } => {
            walk_expr(callee, in_fn, f);
            // the older `sh2.define("name", … => {…})` function shape
            let in_define = is_sh2_call(callee, "define");
            for a in arguments {
                walk_expr(a, if in_define { true } else { in_fn }, f);
            }
        }
        Expr::MemberExpression { object, property, .. } => {
            walk_expr(object, in_fn, f);
            walk_expr(property, in_fn, f);
        }
        Expr::AwaitExpression { argument } => walk_expr(argument, in_fn, f),
        Expr::ArrowFunctionExpression { params, body, .. } => {
            for p in params {
                walk_expr(p, in_fn, f);
            }
            match body {
                ArrowBody::Expr(e) => walk_expr(e, in_fn, f),
                ArrowBody::Block(s) => walk_stmt_exprs(s, in_fn, f),
            }
        }
        Expr::ObjectExpression { properties } => {
            for p in properties {
                walk_expr(&p.key, in_fn, f);
                walk_expr(&p.value, in_fn, f);
            }
        }
        Expr::ArrayExpression { elements } => {
            for el in elements.iter().flatten() {
                walk_expr(el, in_fn, f);
            }
        }
        Expr::SpreadElement { argument } => walk_expr(argument, in_fn, f),
        Expr::LogicalExpression { left, right, .. }
        | Expr::BinaryExpression { left, right, .. } => {
            walk_expr(left, in_fn, f);
            walk_expr(right, in_fn, f);
        }
        Expr::AssignmentExpression { left, right, .. } => {
            // `__fn_f = async () => {…}` — the script-function binding
            // (the current emitter's shape: `let __fn_f = null` then the
            // sequence assigns the arrow). The arrow body is deferred
            // (scope-safety guard).
            let fn_assign = matches!(&**left, Expr::Identifier { name } if name.starts_with("__fn_"))
                && matches!(&**right, Expr::ArrowFunctionExpression { .. });
            walk_expr(left, in_fn, f);
            walk_expr(right, if fn_assign { true } else { in_fn }, f);
        }
        Expr::ConditionalExpression { test, consequent, alternate, .. } => {
            walk_expr(test, in_fn, f);
            walk_expr(consequent, in_fn, f);
            walk_expr(alternate, in_fn, f);
        }
        Expr::UnaryExpression { argument, .. } => walk_expr(argument, in_fn, f),
        Expr::SequenceExpression { expressions } => {
            for x in expressions {
                walk_expr(x, in_fn, f);
            }
        }
        Expr::NewExpression { callee, arguments } => {
            walk_expr(callee, in_fn, f);
            for a in arguments {
                walk_expr(a, in_fn, f);
            }
        }
    }
}

/// Rewrite a provably-static array to native JS (lower.js's
/// lowerNativeArrays, moved into the emitter). Runs FIRST in the
/// post-emission pipeline (mirrors the website's pass order).
pub(crate) fn lower_native_arrays(prog: Program) -> Program {
    // Which names have a DIRECT top-level `sh2.setArray("name", [..])`?
    let mut top_set_arrays: std::collections::HashSet<String> = Default::default();
    for stmt in &prog.body {
        if let Stmt::ExpressionStatement { expression } = stmt {
            if let Expr::CallExpression { callee, arguments, .. } = expression {
                if is_sh2_call(callee, "setArray") {
                    if let [Expr::Literal { value, .. }, Expr::ArrayExpression { .. }] =
                        arguments.as_slice()
                    {
                        if let Some(n) = value.as_str() {
                            top_set_arrays.insert(n.to_string());
                        }
                    }
                }
            }
        }
    }
    // Classify every sh2 call in every top-level statement.
    let mut acc: std::collections::HashMap<String, ArrayRefs> = Default::default();
    let mut declared: std::collections::HashSet<String> = Default::default();
    for (stmt_idx, stmt) in prog.body.iter().enumerate() {
        walk_stmt_exprs(stmt, false, &mut |e, in_fn| {
            if let Expr::CallExpression { callee, arguments, .. } = e {
                if let Some(fn_name) = sh2_callee_name(callee) {
                    classify_array_call(
                        fn_name,
                        arguments,
                        in_fn,
                        stmt_idx,
                        &top_set_arrays,
                        &mut acc,
                    );
                }
            }
            if let Expr::Identifier { name } = e {
                declared.insert(name.clone());
            }
        });
    }
    // Decide: exactly one top-level array-valued setArray; read-only
    // literal-index refs; nothing whole/write/unset/computed/in-function;
    // no other bare use of the name anywhere.
    let natives: std::collections::HashSet<String> = acc
        .iter()
        .filter(|(name, a)| {
            a.set_arrays == 1
                && a.top_set_array
                && a.set_array_idx.is_some()
                && a.read_stmt_idxs
                    .iter()
                    .all(|i| a.set_array_idx.unwrap() < *i)
                && !a.whole
                && !a.writes
                && !a.in_fn
                && !a.index_bad
                && !declared.contains(*name)
        })
        .map(|(n, _)| n.clone())
        .collect();
    if natives.is_empty() {
        return prog;
    }
    // Apply: setArray statement → `let name = items;` (with the runtime
    // setArray's one-level SPLICE — array-valued items are flattened, so
    // the initializer is `[].concat(...items)` unless every item is a
    // plain scalar); rewrite reads everywhere.
    let body = prog
        .body
        .into_iter()
        .map(|stmt| match &stmt {
            Stmt::ExpressionStatement { expression } => {
                if let Expr::CallExpression { callee, arguments, .. } = expression {
                    if is_sh2_call(callee, "setArray") {
                        if let [Expr::Literal { value, .. }, items] = arguments.as_slice() {
                            if let Some(n) = value.as_str() {
                                if natives.contains(n) {
                                    let items: Vec<Expr> = match items {
                                        Expr::ArrayExpression { elements } => elements
                                            .iter()
                                            .flatten()
                                            .map(|e| lower_expr(e.clone(), &natives))
                                            .collect(),
                                        _ => vec![],
                                    };
                                    let init = if items.iter().all(is_scalar_array_item) {
                                        Expr::ArrayExpression {
                                            elements: items.into_iter().map(Some).collect(),
                                        }
                                    } else {
                                        // the runtime splices array-valued
                                        // items one level ([].concat)
                                        Expr::CallExpression {
                                            callee: Box::new(Expr::MemberExpression {
                                                object: Box::new(Expr::ArrayExpression {
                                                    elements: vec![],
                                                }),
                                                property: Box::new(Expr::Identifier {
                                                    name: "concat".to_string(),
                                                }),
                                                computed: false,
                                                optional: false,
                                            }),
                                            arguments: items,
                                            optional: false,
                                        }
                                    };
                                    return Stmt::VariableDeclaration {
                                        kind: "let",
                                        declarations: vec![VariableDeclarator {
                                            type_: "VariableDeclarator",
                                            id: Expr::Identifier {
                                                name: n.to_string(),
                                            },
                                            init: Some(init),
                                        }],
                                    };
                                }
                            }
                        }
                    }
                }
                Stmt::ExpressionStatement {
                    expression: lower_expr(expression.clone(), &natives),
                }
            }
            _ => lower_stmt(stmt.clone(), &natives),
        })
        .collect();
    Program {
        type_: prog.type_,
        source_type: prog.source_type,
        body,
    }
}

/// A setArray item that is provably a plain scalar (a JS array literal
/// element, no runtime one-level splice needed). Calls, identifiers,
/// arrays and member reads may hold ARRAYS — the runtime setArray
/// splices those (`out.push(...e)`), so they take the `[].concat(...)`
/// initializer form instead.
fn is_scalar_array_item(e: &Expr) -> bool {
    match e {
        Expr::Literal { .. }
        | Expr::TemplateLiteral { .. }
        | Expr::UnaryExpression { .. }
        | Expr::BinaryExpression { .. }
        | Expr::LogicalExpression { .. }
        | Expr::ConditionalExpression { .. }
        | Expr::SequenceExpression { .. }
        | Expr::AssignmentExpression { .. } => true,
        _ => false,
    }
}

fn lower_stmt(stmt: Stmt, natives: &std::collections::HashSet<String>) -> Stmt {
    match stmt {
        Stmt::ExpressionStatement { expression } => Stmt::ExpressionStatement {
            expression: lower_expr(expression, natives),
        },
        Stmt::BlockStatement { body } => Stmt::BlockStatement {
            body: body.into_iter().map(|s| lower_stmt(s, natives)).collect(),
        },
        Stmt::IfStatement { test, consequent, alternate } => Stmt::IfStatement {
            test: lower_expr(test, natives),
            consequent: Box::new(lower_stmt(*consequent, natives)),
            alternate: alternate.map(|a| Box::new(lower_stmt(*a, natives))),
        },
        Stmt::SwitchStatement { discriminant, cases } => Stmt::SwitchStatement {
            discriminant: lower_expr(discriminant, natives),
            cases: cases
                .into_iter()
                .map(|c| SwitchCase {
                    type_: c.type_,
                    test: c.test.map(|t| lower_expr(t, natives)),
                    consequent: c
                        .consequent
                        .into_iter()
                        .map(|s| lower_stmt(s, natives))
                        .collect(),
                })
                .collect(),
        },
        Stmt::WhileStatement { test, body } => Stmt::WhileStatement {
            test: lower_expr(test, natives),
            body: Box::new(lower_stmt(*body, natives)),
        },
        Stmt::TryStatement {
            block,
            handler,
            finalizer,
        } => Stmt::TryStatement {
            block: Box::new(lower_stmt(*block, natives)),
            handler: handler.map(|h| CatchClause {
                type_: h.type_,
                param: h.param.map(|p| Box::new(lower_expr(*p, natives))),
                body: Box::new(lower_stmt(*h.body, natives)),
            }),
            finalizer: finalizer.map(|f| Box::new(lower_stmt(*f, natives))),
        },
        Stmt::ForStatement { init, test, update, body } => Stmt::ForStatement {
            init: Box::new(lower_stmt(*init, natives)),
            test: lower_expr(test, natives),
            update: lower_expr(update, natives),
            body: Box::new(lower_stmt(*body, natives)),
        },
        Stmt::ForOfStatement { left, right, body } => Stmt::ForOfStatement {
            left: Box::new(lower_stmt(*left, natives)),
            right: lower_expr(right, natives),
            body: Box::new(lower_stmt(*body, natives)),
        },
        Stmt::VariableDeclaration { declarations, kind } => Stmt::VariableDeclaration {
            declarations: declarations
                .into_iter()
                .map(|d| VariableDeclarator {
                    type_: d.type_,
                    id: lower_expr(d.id, natives),
                    init: d.init.map(|i| lower_expr(i, natives)),
                })
                .collect(),
            kind,
        },
        Stmt::BreakStatement { label } => Stmt::BreakStatement { label },
        Stmt::ContinueStatement { label } => Stmt::ContinueStatement { label },
        Stmt::ReturnStatement { argument } => Stmt::ReturnStatement {
            argument: argument.map(|a| lower_expr(a, natives)),
        },
        Stmt::ThrowStatement { argument } => Stmt::ThrowStatement {
            argument: lower_expr(argument, natives),
        },
    }
}

/// Replace a matching sh2 read call with its native form; recurse.
fn lower_expr(e: Expr, natives: &std::collections::HashSet<String>) -> Expr {
    if let Expr::CallExpression { callee, arguments, .. } = &e {
        if let Some(fn_name) = sh2_callee_name(callee) {
            match fn_name {
                "arrayIndex" | "getVar" => {
                    if let Some((name, idx)) = array_read_index(fn_name, arguments) {
                        if natives.contains(name) {
                            if let Some(idx) = idx {
                                return native_element_read(name, idx);
                            }
                        }
                    }
                }
                "arrayLen" | "arrayItems" | "param" => {
                    if let Some((name, len)) = array_len_join(fn_name, arguments) {
                        if natives.contains(name) {
                            return if len {
                                native_len_read(name)
                            } else {
                                Expr::Identifier { name: name.to_string() }
                            };
                        }
                    }
                }
                _ => {}
            }
        }
    }
    match e {
        Expr::CallExpression { callee, arguments, optional } => Expr::CallExpression {
            callee: Box::new(lower_expr(*callee, natives)),
            arguments: arguments
                .into_iter()
                .map(|a| lower_expr(a, natives))
                .collect(),
            optional,
        },
        Expr::Identifier { .. } | Expr::Literal { .. } => e,
        Expr::TemplateLiteral { quasis, expressions } => Expr::TemplateLiteral {
            quasis,
            expressions: expressions
                .into_iter()
                .map(|x| lower_expr(x, natives))
                .collect(),
        },
        Expr::MemberExpression { object, property, computed, optional } => Expr::MemberExpression {
            object: Box::new(lower_expr(*object, natives)),
            property: Box::new(lower_expr(*property, natives)),
            computed,
            optional,
        },
        Expr::AwaitExpression { argument } => Expr::AwaitExpression {
            argument: Box::new(lower_expr(*argument, natives)),
        },
        Expr::ArrowFunctionExpression { params, body, expression, r#async } => {
            Expr::ArrowFunctionExpression {
                params: params.into_iter().map(|p| lower_expr(p, natives)).collect(),
                body: match body {
                    ArrowBody::Expr(e) => ArrowBody::Expr(Box::new(lower_expr(*e, natives))),
                    ArrowBody::Block(s) => ArrowBody::Block(Box::new(lower_stmt(*s, natives))),
                },
                expression,
                r#async,
            }
        }
        Expr::ObjectExpression { properties } => Expr::ObjectExpression {
            properties: properties
                .into_iter()
                .map(|p| Property {
                    type_: p.type_,
                    key: lower_expr(p.key, natives),
                    value: lower_expr(p.value, natives),
                    kind: p.kind,
                    computed: p.computed,
                    shorthand: p.shorthand,
                })
                .collect(),
        },
        Expr::ArrayExpression { elements } => Expr::ArrayExpression {
            elements: elements
                .into_iter()
                .map(|el| el.map(|x| lower_expr(x, natives)))
                .collect(),
        },
        Expr::SpreadElement { argument } => Expr::SpreadElement {
            argument: Box::new(lower_expr(*argument, natives)),
        },
        Expr::LogicalExpression { operator, left, right } => Expr::LogicalExpression {
            operator,
            left: Box::new(lower_expr(*left, natives)),
            right: Box::new(lower_expr(*right, natives)),
        },
        Expr::BinaryExpression { operator, left, right } => Expr::BinaryExpression {
            operator,
            left: Box::new(lower_expr(*left, natives)),
            right: Box::new(lower_expr(*right, natives)),
        },
        Expr::AssignmentExpression { operator, left, right } => Expr::AssignmentExpression {
            operator,
            left: Box::new(lower_expr(*left, natives)),
            right: Box::new(lower_expr(*right, natives)),
        },
        Expr::ConditionalExpression { test, consequent, alternate } => Expr::ConditionalExpression {
            test: Box::new(lower_expr(*test, natives)),
            consequent: Box::new(lower_expr(*consequent, natives)),
            alternate: Box::new(lower_expr(*alternate, natives)),
        },
        Expr::UnaryExpression { operator, argument, prefix } => Expr::UnaryExpression {
            operator,
            argument: Box::new(lower_expr(*argument, natives)),
            prefix,
        },
        Expr::SequenceExpression { expressions } => Expr::SequenceExpression {
            expressions: expressions.into_iter().map(|x| lower_expr(x, natives)).collect(),
        },
        Expr::NewExpression { callee, arguments } => Expr::NewExpression {
            callee: Box::new(lower_expr(*callee, natives)),
            arguments: arguments.into_iter().map(|a| lower_expr(a, natives)).collect(),
        },
    }
}

/// `sh2.getVar("arr[1]")` / `sh2.arrayIndex("arr", "1")` → (name, Some(idx))
/// for a LITERAL non-negative integer index; None for anything else.
fn array_read_index<'a>(fn_name: &str, args: &'a [Expr]) -> Option<(&'a str, Option<i64>)> {
    let name = lit_str(args.first()?)?;
    if fn_name == "getVar" {
        let (n, idx) = parse_var_arg_str(name)?;
        let idx = idx?;
        let v = idx.parse::<i64>().ok()?;
        (v >= 0).then_some((n, Some(v)))
    } else {
        let v = args.get(1)?.as_literal_i64()?;
        (v >= 0).then_some((name, Some(v)))
    }
}

/// `sh2.arrayLen("arr")` / `arrayItems("arr")` / `param("slice", …)` →
/// (name, is_len). `arrayItems` returns the ARRAY (the native echo
/// wraps it in `[].concat(…).join(" ")`), so the rewrite is the bare
/// identifier; `len` rewrites to `name.length`.
fn array_len_join<'a>(fn_name: &str, args: &'a [Expr]) -> Option<(&'a str, bool)> {
    match fn_name {
        "arrayLen" => lit_str(args.first()?).map(|n| (n, true)),
        "arrayItems" => lit_str(args.first()?).map(|n| (n, false)),
        "param" => {
            let op = lit_str(args.first()?)?;
            let mode = args.get(2).and_then(lit_str)?;
            if op != "slice" || mode != "@" {
                return None;
            }
            let target = lit_str(args.get(1)?)?;
            Some((target.trim_start_matches('#'), target.starts_with('#')))
        }
        _ => None,
    }
}

fn native_element_read(name: &str, idx: i64) -> Expr {
    let elem = Expr::MemberExpression {
        object: Box::new(Expr::Identifier {
            name: name.to_string(),
        }),
        property: Box::new(Expr::Literal {
            value: serde_json::Value::from(idx),
            raw: None,
            regex: None,
        }),
        computed: true,
        optional: false,
    };
    Expr::ConditionalExpression {
        test: Box::new(Expr::BinaryExpression {
            operator: "!==".to_string(),
            left: Box::new(elem.clone()),
            right: Box::new(Expr::Identifier {
                name: "undefined".to_string(),
            }),
        }),
        consequent: Box::new(elem),
        alternate: Box::new(Expr::Literal {
            value: serde_json::Value::String(String::new()),
            raw: None,
            regex: None,
        }),
    }
}

fn native_len_read(name: &str) -> Expr {
    Expr::MemberExpression {
        object: Box::new(Expr::Identifier {
            name: name.to_string(),
        }),
        property: Box::new(Expr::Identifier {
            name: "length".to_string(),
        }),
        computed: false,
        optional: false,
    }
}

trait AsLiteralI64 {
    fn as_literal_i64(&self) -> Option<i64>;
}
impl AsLiteralI64 for Expr {
    fn as_literal_i64(&self) -> Option<i64> {
        match self {
            // The emitter passes bash index TEXT (arrayIndex("arr",
            // "1")) — accept both JSON numbers and numeric strings.
            Expr::Literal { value, .. } => value
                .as_i64()
                .or_else(|| value.as_str().and_then(|s| s.parse::<i64>().ok())),
            _ => None,
        }
    }
}

// ── dead-flag removal ───────────────────────────────────────────────
//
// Migrated from the website's src/lower.js `dropDeadFlags`. A command
// statement's VALUE is its success flag — `(cmd?, flag)` — consumed ONLY
// for the program's last statement (jtsh's runViaTranspiler returns it
// as the exit code; the harness `_finish` reads sh2.lastExit, which the
// last statement's status lowering set). Every other statement — loop
// bodies, blocks, branches, guarded calls' args — has a dead value, so
// `(cmd, true)` is just `cmd`. Also unwraps 1-element sequences (a bare
// `(flag)` left after the lastExit hoist) and drops literal-only
// statements (no side effects).
//
// Runs AFTER the lastExit hoist (mirrors the website's pass order). The
// trailing element is popped ONLY when it is PURE — an if-statement
// lowers to `(test, lastExit === 0 ? BRANCH : false)` whose tail is the
// side-effecting BRANCH conditional (writes, calls); popping that would
// DELETE the branch, so purity is the guard.

fn is_pure_expr(e: &Expr) -> bool {
    match e {
        Expr::Literal { .. } | Expr::Identifier { .. } => true,
        Expr::TemplateLiteral { expressions, .. } => expressions.iter().all(is_pure_expr),
        Expr::UnaryExpression { argument, .. } => is_pure_expr(argument),
        Expr::BinaryExpression { left, right, .. } | Expr::LogicalExpression { left, right, .. } => {
            is_pure_expr(left) && is_pure_expr(right)
        }
        Expr::ConditionalExpression { test, consequent, alternate } => {
            is_pure_expr(test) && is_pure_expr(consequent) && is_pure_expr(alternate)
        }
        _ => false,
    }
}

/// Process one statement (drop its trailing flag / unwrap / mark dead).
/// Returns true when the statement is a bare literal (drop it).
fn drop_stmt_flags(stmt: &mut Stmt) -> bool {
    if let Stmt::ExpressionStatement { expression } = stmt {
        if let Expr::SequenceExpression { expressions } = expression {
            if expressions.len() == 1 {
                if is_pure_literal(&expressions[0]) {
                    return true; // a bare `(true)` — no side effects
                }
                *expression = expressions.pop().unwrap();
            } else if is_pure_expr(expressions.last().unwrap()) {
                expressions.pop();
                if expressions.len() == 1 {
                    *expression = expressions.pop().unwrap();
                }
            }
        }
    }
    drop_nested_flags(stmt);
    false
}

/// Recurse into a statement's nested statement lists (no exemption).
fn drop_nested_flags(stmt: &mut Stmt) {
    match stmt {
        Stmt::ExpressionStatement { expression } => drop_expr_flags(expression),
        Stmt::BlockStatement { body } => drop_flags_in_list(body),
        Stmt::IfStatement { test, consequent, alternate } => {
            drop_expr_flags(test);
            drop_stmt_flags(consequent);
            if let Some(a) = alternate {
                drop_stmt_flags(a);
            }
        }
        Stmt::SwitchStatement { discriminant, cases } => {
            drop_expr_flags(discriminant);
            for c in cases {
                drop_flags_in_list(&mut c.consequent);
            }
        }
        Stmt::WhileStatement { test, body } => {
            drop_expr_flags(test);
            drop_stmt_flags(body);
        }
        Stmt::TryStatement {
            block,
            handler,
            finalizer,
        } => {
            drop_stmt_flags(block);
            if let Some(h) = handler {
                drop_stmt_flags(&mut h.body);
            }
            if let Some(f) = finalizer {
                drop_stmt_flags(f);
            }
        }
        Stmt::ThrowStatement { argument } => drop_expr_flags(argument),
        Stmt::ForStatement { init, test, update, body } => {
            drop_stmt_flags(init);
            drop_expr_flags(test);
            drop_expr_flags(update);
            drop_stmt_flags(body);
        }
        Stmt::ForOfStatement { left, right, body } => {
            drop_stmt_flags(left);
            drop_expr_flags(right);
            drop_stmt_flags(body);
        }
        Stmt::VariableDeclaration { declarations, .. } => {
            for d in declarations {
                drop_expr_flags(&mut d.id);
                if let Some(i) = &mut d.init {
                    drop_expr_flags(i);
                }
            }
        }
        Stmt::BreakStatement { .. } | Stmt::ContinueStatement { .. } => {}
        Stmt::ReturnStatement { argument } => {
            if let Some(a) = argument {
                drop_expr_flags(a);
            }
        }
    }
}

/// Recurse through an expression's nested statement lists (arrow bodies).
fn drop_expr_flags(e: &mut Expr) {
    match e {
        Expr::Identifier { .. } | Expr::Literal { .. } => {}
        Expr::TemplateLiteral { expressions, .. } => {
            for x in expressions {
                drop_expr_flags(x);
            }
        }
        Expr::CallExpression { callee, arguments, .. } => {
            drop_expr_flags(callee);
            for a in arguments {
                drop_expr_flags(a);
            }
        }
        Expr::MemberExpression { object, property, .. } => {
            drop_expr_flags(object);
            drop_expr_flags(property);
        }
        Expr::AwaitExpression { argument } => drop_expr_flags(argument),
        Expr::ArrowFunctionExpression { body, .. } => match body {
            ArrowBody::Expr(e) => drop_expr_flags(e),
            ArrowBody::Block(s) => drop_nested_flags(s),
        },
        Expr::ObjectExpression { properties } => {
            for p in properties {
                drop_expr_flags(&mut p.key);
                drop_expr_flags(&mut p.value);
            }
        }
        Expr::ArrayExpression { elements } => {
            for el in elements.iter_mut().flatten() {
                drop_expr_flags(el);
            }
        }
        Expr::SpreadElement { argument } => drop_expr_flags(argument),
        Expr::LogicalExpression { left, right, .. }
        | Expr::BinaryExpression { left, right, .. }
        | Expr::AssignmentExpression { left, right, .. } => {
            drop_expr_flags(left);
            drop_expr_flags(right);
        }
        Expr::ConditionalExpression { test, consequent, alternate } => {
            drop_expr_flags(test);
            drop_expr_flags(consequent);
            drop_expr_flags(alternate);
        }
        Expr::UnaryExpression { argument, .. } => drop_expr_flags(argument),
        Expr::SequenceExpression { expressions } => {
            for x in expressions {
                drop_expr_flags(x);
            }
        }
        Expr::NewExpression { callee, arguments } => {
            drop_expr_flags(callee);
            for a in arguments {
                drop_expr_flags(a);
            }
        }
    }
}

/// Remove unconsumed success flags from a statement list (no exemption).
fn drop_flags_in_list(stmts: &mut Vec<Stmt>) {
    let n = stmts.len();
    let mut dead = vec![false; n];
    for i in 0..n {
        dead[i] = drop_stmt_flags(&mut stmts[i]);
    }
    if dead.iter().any(|d| *d) {
        let mut j = 0;
        stmts.retain(|_| {
            let d = dead[j];
            j += 1;
            !d
        });
    }
}

/// The program's last statement's VALUE is the exit flag — it keeps its
/// sequence. Every other statement (all levels) loses its dead flag.
pub(crate) fn drop_dead_flags(prog: Program) -> Program {
    let mut body = prog.body;
    let n = body.len();
    let mut dead = vec![false; n];
    for i in 0..n {
        if i == n - 1 {
            drop_nested_flags(&mut body[i]); // keep its own value; recurse
        } else {
            dead[i] = drop_stmt_flags(&mut body[i]);
        }
    }
    if dead.iter().any(|d| *d) {
        let mut j = 0;
        body.retain(|_| {
            let d = dead[j];
            j += 1;
            !d
        });
    }
    Program {
        type_: prog.type_,
        source_type: prog.source_type,
        body,
    }
}

// ── process-substitution pre-pass ───────────────────────────────────
//
// The parser stores `<(cmd)` both as an argument position (the argument
// itself is dropped, the redirect carries the inner command) and as an
// explicit stdin redirect (`cmd < <(cmd)`). Both arrive as
// `RedirectOperator::ProcessSubstitutionInput`. We rewrite:
//
//   1. the redirect → a here-string whose content is the inner command's
//      captured stdout (`sh2.capture`), so stdin-based consumers
//      (`mapfile`, `while ...; done < <(...)`) see the produced stream;
//   2. for ordinary commands, additionally append a magic-prefixed
//      argument carrying the same capture; the runtime (`sh2.exec`)
//      recognizes the prefix and materializes it to a temp file path,
//      emulating bash's `/dev/fd/N` argument passing (`diff <(a) <(b)`).
//
// `mapfile`/`readarray` are stdin-based only: appending a path argument
// would be an error, so they get the here-string rewrite alone.

/// Marker prefix the runtime recognizes on exec arguments (see
/// sh2-namespace.mjs: `exec` materializes the suffix to a temp file).
const PS_MAGIC: &str = "\u{1}SH2PS\u{1}";

/// Commands that read stdin when given no file arguments. For these the
/// process-substitution rewrite adds ONLY the here-string (captured producer
/// stdout on stdin); appending a materialized-path argument would duplicate
/// the capture (`head <(while true; do echo .; sleep 1; done)` would run the
/// producer twice — 2× the capture time bound — and time out the gate).
fn stdin_only_command(name: Option<&str>) -> bool {
    matches!(
        name,
        Some("mapfile")
            | Some("readarray")
            | Some("head")
            | Some("tail")
            | Some("cat")
            | Some("wc")
    )
}

fn transform_cmd(cmd: &Command) -> Command {
    match cmd {
        // Unterminated-parameter-expansion artifact: the lexer reads
        // `echo "${var:?unset"` (missing closing `}`) as a bare
        // StringInterpolation whose literal parts re-join to text starting
        // with `${`. bash rejects the construct at parse time and aborts the
        // whole script there, so the word can never execute: drop the
        // command (BlankLine lowers to nothing) instead of printing the raw
        // artifact text.
        Command::Simple(_) if command_is_unterminated_param_artifact(cmd) => Command::BlankLine,
        Command::BuiltinCommand(_) if command_is_unterminated_param_artifact(cmd) => {
            Command::BlankLine
        }
        Command::Simple(sc) => {
            let mut sc = sc.clone();
            let ps = transform_redirects(&mut sc.redirects);
            if !ps.is_empty() {
                let stdin_only = stdin_only_command(sc.name.as_literal());
                if !stdin_only {
                    // Re-add the dropped argument positions as materialized
                    // paths (the parser kept only the redirects).
                    for inner in ps {
                        sc.args.push(ps_arg_word(inner));
                    }
                }
            }
            sc.name = transform_word(sc.name);
            sc.args = sc.args.drain(..).map(transform_word).collect();
            Command::Simple(sc)
        }
        Command::BuiltinCommand(bc) => {
            let mut bc = bc.clone();
            let ps = transform_redirects(&mut bc.redirects);
            if !ps.is_empty() {
                let stdin_only = stdin_only_command(Some(&bc.name));
                if !stdin_only {
                    for inner in ps {
                        bc.args.push(ps_arg_word(inner));
                    }
                }
            }
            bc.args = bc.args.drain(..).map(transform_word).collect();
            Command::BuiltinCommand(bc)
        }
        Command::Redirect(rc) => {
            let mut rc = rc.clone();
            let producers = transform_redirects(&mut rc.redirects);
            rc.command = Box::new(transform_cmd(&rc.command));
            // Re-add dropped argument positions (`diff <(a) <(b)`): the
            // parser lifts `<(...)` args onto the redirect list, so the
            // inner simple command must get the materialized paths back.
            if !producers.is_empty() {
                append_ps_args(&mut rc.command, producers);
            }
            Command::Redirect(rc)
        }
        Command::Pipeline(p) => {
            let mut p = p.clone();
            p.commands = p.commands.iter().map(transform_cmd).collect();
            Command::Pipeline(p)
        }
        Command::And(l, r) => Command::And(Box::new(transform_cmd(l)), Box::new(transform_cmd(r))),
        Command::Or(l, r) => Command::Or(Box::new(transform_cmd(l)), Box::new(transform_cmd(r))),
        Command::Not(c) => Command::Not(Box::new(transform_cmd(c))),
        Command::Background(c) => Command::Background(Box::new(transform_cmd(c))),
        Command::Subshell(c) => Command::Subshell(Box::new(transform_cmd(c))),
        Command::If(i) => {
            let mut i = i.clone();
            i.condition = Box::new(transform_cmd(&i.condition));
            i.then_branch = Box::new(transform_cmd(&i.then_branch));
            i.else_branch = i.else_branch.map(|b| Box::new(transform_cmd(&b)));
            Command::If(i)
        }
        Command::Case(c) => {
            let mut c = c.clone();
            c.word = transform_word(c.word);
            for cl in &mut c.cases {
                cl.patterns = cl.patterns.iter().cloned().map(transform_word).collect();
                cl.body = cl.body.iter().map(transform_cmd).collect();
            }
            Command::Case(c)
        }
        Command::While(w) => {
            let mut w = w.clone();
            w.condition = Box::new(transform_cmd(&w.condition));
            w.body = Block {
                commands: w.body.commands.iter().map(transform_cmd).collect(),
            };
            Command::While(w)
        }
        Command::For(f) => {
            let mut f = f.clone();
            f.items = f.items.iter().cloned().map(transform_word).collect();
            f.body = Block {
                commands: f.body.commands.iter().map(transform_cmd).collect(),
            };
            Command::For(f)
        }
        Command::Function(f) => {
            let mut f = f.clone();
            f.body = Block {
                commands: f.body.commands.iter().map(transform_cmd).collect(),
            };
            Command::Function(f)
        }
        Command::Block(b) => Command::Block(Block {
            commands: b.commands.iter().map(transform_cmd).collect(),
        }),
        Command::Assignment(a) => {
            let mut a = a.clone();
            a.value = transform_word(a.value);
            Command::Assignment(a)
        }
        Command::Return(w) => Command::Return(w.as_ref().map(|w| transform_word(w.clone()))),
        other => other.clone(),
    }
}

/// True when `w` is the lexer's artifact for an unterminated parameter
/// expansion inside a double-quoted string: a pure-literal interpolation
/// (or bare literal) that starts with `${`, has content beyond the `${`,
/// and never closes the brace. Legit single-quoted `'${x}'` text closes the
/// brace; a bare `'${'` (escaped-dollar artifact, `\${`) is only the
/// two-char opener — both excluded.
fn is_unterminated_param_literal(w: &Word) -> bool {
    let joined = match w {
        Word::StringInterpolation(interp, _)
            if interp
                .parts
                .iter()
                .all(|p| matches!(p, StringPart::Literal(_))) =>
        {
            interp
                .parts
                .iter()
                .filter_map(|p| match p {
                    StringPart::Literal(s) => Some(s.as_str()),
                    _ => None,
                })
                .collect::<String>()
        }
        Word::Literal(s, _) => s.clone(),
        _ => return false,
    };
    joined.starts_with("${") && joined.len() > 2 && !joined.contains('}')
}

/// A simple/builtin command whose NAME or any ARG is such an artifact word.
fn command_is_unterminated_param_artifact(cmd: &Command) -> bool {
    match cmd {
        Command::Simple(sc) => {
            is_unterminated_param_literal(&sc.name)
                || sc.args.iter().any(is_unterminated_param_literal)
        }
        Command::BuiltinCommand(bc) => bc.args.iter().any(is_unterminated_param_literal),
        _ => false,
    }
}

/// The lexer's LongOption handler merges a following quoted string into the
/// option text as RAW text: `--x="${X}"` arrives as the bare literal
/// `--x=${X}` with the parameter expansion lost (the corpus test
/// parse-longoption-with-dollar.sh documents this). An escaped `\${` keeps
/// its backslash, so unescaped `${` is only ambiguous with single-quoted
/// text; the corpus has no such literal, and requiring an unescaped `=`
/// BEFORE the `${` AND a leading `-` (the LongOption artifact is always
/// `--word=${...}`; `echo '${x}'` (no `=`), `x="${X}"` (parsed as an
/// interpolation already), and multi-line single-quoted script text (no
/// leading dash — readonly-cmdsub.sh) are all excluded.)
/// Split such literals into interpolation parts so the expansion runs.
fn split_literal_params(s: &str) -> Option<Word> {
    let mut parts: Vec<StringPart> = Vec::new();
    let mut lit = String::new();
    let mut changed = false;
    let mut seen_eq = false;
    let chars: Vec<(usize, char)> = s.char_indices().collect();
    let mut i = 0;
    while i < chars.len() {
        let (pos, c) = chars[i];
        if c == '$'
            && pos + 1 < s.len()
            && s.as_bytes()[pos + 1] == b'{'
            && (pos == 0 || s.as_bytes()[pos - 1] != b'\\')
            && seen_eq
            && s.starts_with('-')
        {
            if let Some(close) = s[pos + 2..].find('}') {
                let name = &s[pos + 2..pos + 2 + close];
                if is_plain_param_name(name) {
                    if !lit.is_empty() {
                        parts.push(StringPart::Literal(std::mem::take(&mut lit)));
                    }
                    parts.push(StringPart::ParameterExpansion(ParameterExpansion {
                        variable: name.to_string(),
                        operator: ParameterExpansionOperator::None,
                        is_mutable: false,
                    }));
                    changed = true;
                    // advance past `${name}`
                    let consumed = 2 + close + 1;
                    i = 0;
                    while i < chars.len() && chars[i].0 < pos + consumed {
                        i += 1;
                    }
                    continue;
                }
            }
        }
        if c == '=' {
            seen_eq = true;
        }
        lit.push(c);
        i += 1;
    }
    if !changed {
        return None;
    }
    if !lit.is_empty() {
        parts.push(StringPart::Literal(lit));
    }
    Some(Word::StringInterpolation(
        StringInterpolation { parts },
        None,
    ))
}

/// A bare `${...}` reference with no operator: plain identifier, positional
/// number, or a single special-parameter char. Anything else (operators like
/// `:-`, array slices) stays literal — the lexer normally parses those as
/// ParameterExpansion words already, so an artifact literal never has them.
fn is_plain_param_name(name: &str) -> bool {
    if name.is_empty() {
        return false;
    }
    if name.len() == 1 && "@*#?$!-".contains(name) {
        return true;
    }
    let mut cs = name.chars();
    let first = cs.next().unwrap();
    (first.is_ascii_alphabetic() || first == '_' || first.is_ascii_digit())
        && cs.all(|c| c.is_ascii_alphanumeric() || c == '_')
}

fn transform_word(w: Word) -> Word {
    match w {
        Word::CommandSubstitution(inner, ann) => {
            Word::CommandSubstitution(Box::new(transform_cmd(&inner)), ann)
        }
        Word::StringInterpolation(mut interp, ann) => {
            let mut parts = Vec::with_capacity(interp.parts.len());
            for p in interp.parts {
                match p {
                    StringPart::CommandSubstitution(inner) => parts.push(
                        StringPart::CommandSubstitution(Box::new(transform_cmd(&inner))),
                    ),
                    other => parts.push(other),
                }
            }
            interp.parts = parts;
            Word::StringInterpolation(interp, ann)
        }
        // Quoted literals (ann == Some — the parser's quote-state marker)
        // are never split: `'--x=${X}'` stays literal text. Only the bare
        // LongOption lexer artifact (`--x=${X}`) is re-split.
        Word::Literal(s, Some(())) => Word::Literal(s, Some(())),
        Word::Literal(s, None) => split_literal_params(&s).unwrap_or(Word::Literal(s, None)),
        other => other,
    }
}

/// Rewrite process-substitution redirects in place; returns the inner
/// commands (in order) so the caller can rebuild argument positions.
fn transform_redirects(redirects: &mut Vec<Redirect>) -> Vec<Command> {
    let mut producers = Vec::new();
    for r in redirects.iter_mut() {
        match std::mem::replace(&mut r.operator, RedirectOperator::Input) {
            RedirectOperator::ProcessSubstitutionInput(inner) => {
                // Recursively transform the producer (it may itself contain
                // `<(sort <(grep ...))` — nested process substitution).
                let inner = transform_cmd(&inner);
                producers.push(inner.clone());
                // here-string: content = captured stdout of the producer
                r.operator = RedirectOperator::HereString;
                r.target = Word::StringInterpolation(
                    StringInterpolation {
                        parts: vec![StringPart::CommandSubstitution(Box::new(inner))],
                    },
                    None,
                );
                r.heredoc_body = None;
                r.heredoc_quoted = true;
            }
            other => r.operator = other,
        }
    }
    producers
}

/// Append one materialized-path argument per producer to a simple/builtin
/// command (skipping stdin-only readers like mapfile). Recurses through
/// nested Redirect wrappers; other command kinds take no arguments.
fn append_ps_args(cmd: &mut Command, producers: Vec<Command>) {
    match cmd {
        Command::Simple(sc) => {
            let stdin_only = stdin_only_command(sc.name.as_literal());
            if !stdin_only {
                for inner in producers {
                    sc.args.push(ps_arg_word(inner));
                }
            }
        }
        Command::BuiltinCommand(bc) => {
            if !stdin_only_command(Some(&bc.name)) {
                for inner in producers {
                    bc.args.push(ps_arg_word(inner));
                }
            }
        }
        Command::Redirect(rc) => append_ps_args(&mut rc.command, producers),
        _ => {}
    }
}

/// Argument word that carries the producer; the runtime materializes the
/// `PS_MAGIC`-prefixed string into a temp file path at exec time.
fn ps_arg_word(inner: Command) -> Word {
    Word::StringInterpolation(
        StringInterpolation {
            parts: vec![
                StringPart::Literal(PS_MAGIC.to_string()),
                StringPart::CommandSubstitution(Box::new(inner)),
            ],
        },
        None,
    )
}

// ── sh2.* namespace helpers ─────────────────────────────────────────

pub(crate) fn sh2_member(name: &str) -> Expr {
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

pub(crate) fn sh2_call(name: &str, args: Vec<Expr>) -> Expr {
    Expr::CallExpression {
        callee: Box::new(sh2_member(name)),
        arguments: args,
        optional: false,
    }
}

pub(crate) fn str_lit(s: &str) -> Expr {
    Expr::Literal {
        value: serde_json::Value::String(s.to_string()),
        raw: None,
        regex: None,
    }
}

/// `obj.name(args...)` — a method call on an arbitrary object expression
/// (the native string-op chains of the capture lifts).
pub(crate) fn method_call(obj: Expr, name: &str, args: Vec<Expr>) -> Expr {
    Expr::CallExpression {
        callee: Box::new(Expr::MemberExpression {
            object: Box::new(obj),
            property: Box::new(Expr::Identifier {
                name: name.to_string(),
            }),
            computed: false,
            optional: false,
        }),
        arguments: args,
        optional: false,
    }
}

/// A bare identifier expression.
pub(crate) fn ident(name: &str) -> Expr {
    Expr::Identifier {
        name: name.to_string(),
    }
}

/// An integer literal expression.
pub(crate) fn int_lit_expr(i: i64) -> Expr {
    Expr::Literal {
        value: serde_json::Value::from(i),
        raw: None,
        regex: None,
    }
}

/// A JS regex literal (`/\s+/`) — the ESTree `Literal`-with-`regex` shape
/// (value: {} like the spec's RegExp literal). Printed by estree-gen.mjs
/// as `/pattern/flags`.
pub(crate) fn regex_lit(pattern: &str) -> Expr {
    regex_lit_flags(pattern, "")
}

/// A JS regex literal with explicit flags (`/\u0000/g` — the trimCapture
/// NUL strip needs the global flag).
pub(crate) fn regex_lit_flags(pattern: &str, flags: &str) -> Expr {
    Expr::Literal {
        value: serde_json::Value::Object(serde_json::Map::new()),
        raw: None,
        regex: Some(RegexLiteral {
            pattern: pattern.to_string(),
            flags: flags.to_string(),
        }),
    }
}

pub(crate) fn prop(key: &str, value: Expr) -> Property {
    Property {
        type_: "Property",
        key: Expr::Identifier {
            name: key.to_string(),
        },
        value,
        kind: "init",
        computed: false,
        shorthand: false,
    }
}

pub(crate) fn quasi_element(raw: &mut String, tail: bool) -> TemplateElement {
    let r = std::mem::take(raw);
    TemplateElement {
        type_: "TemplateElement",
        value: TemplateElementValue {
            raw: escape_template_raw(&r),
            cooked: Some(r),
        },
        tail,
    }
}

/// Escape the literal text of a template-literal quasi. The JS emitter
/// (estree.js → astring) writes `TemplateElement.value.raw` VERBATIM, so
/// the three characters that are special inside a template literal must
/// be escaped: `\` (escape leader), `` ` `` (the delimiter) and `$`
/// (`${` would open an expression slot). `cooked` keeps the unescaped
/// VALUE. Without this, `echo "a\\b$y"` (bash) and `echo %X%\\`
/// (batch) emitted `\b`/a bare trailing `\` into the template — a
/// backspace or an unterminated template literal at eval time.
pub(crate) fn escape_template_raw(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for ch in s.chars() {
        match ch {
            '\\' => out.push_str("\\\\"),
            '`' => out.push_str("\\`"),
            '$' => out.push_str("\\$"),
            _ => out.push(ch),
        }
    }
    out
}

// ── Tests ───────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Parser;

    fn to_json(input: &str) -> String {
        let commands = Parser::new(input).parse().unwrap();
        serde_json::to_string(&ast_to_estree(&commands)).unwrap()
    }

    #[test]
    fn if_empty_else_lastexit_dropped_when_unread() {
        // `if c; then ...; fi` with NO else synthesizes a false-path
        // `sh2.lastExit = 0` (bash: false cond + no else → $? = 0). The
        // Plan 4 liveness marks it dead when nothing reads the if's
        // status — the if lowers to a plain `if (c) { ... }`, no else.
        // NOTE: the PROGRAM-FINAL status IS a reader now (the runner's
        // `sh2._finish()` exits with `sh2.lastExit` — bash's exit code is
        // the last command's status and the corpus gate compares exit
        // codes), so a program-final if KEEPS its false-path write.
        // The lastExit-tail hoist then LIFTS that write after the if —
        // semantically identical (the reader still sees 0), structurally
        // a post-if `sh2.lastExit = 0` instead of an else branch.
        let json = to_json("if false; then echo yes; fi");
        assert!(json.contains("\"type\":\"IfStatement\""));
        assert!(
            json.contains("\"alternate\":null") && json.contains("\"name\":\"lastExit\""),
            "program-final status read → the false-path write is lifted after the if"
        );
        assert!(!json.contains("unsupported"));
        // a READER keeps the write: `; echo $?` observes the false-path 0
        // — the hoisted post-if write still precedes the reader
        let json2 = to_json("if false; then echo yes; fi; echo $?");
        assert!(
            json2.contains("\"name\":\"lastExit\""),
            "read status → write kept (lifted after the if)"
        );
        assert!(!json2.contains("unsupported"));
        // a later WRITER shadows the if's status → the write is dead again
        let json3 = to_json("if false; then echo yes; fi; false; echo $?");
        assert!(
            json3.contains("\"alternate\":null"),
            "shadowed by `false` → no else"
        );
        assert!(!json3.contains("unsupported"));
    }

    #[test]
    fn echo_lowers_to_builtin_call() {
        // `echo` with literal args at the default stdout sink → a NATIVE
        // `process.stdout.write` sequence (no dispatch at all); echo args
        // the runtime would transform (globs, process substitutions) keep
        // the sync builtin call (`sh2.builtin("echo", ...)`, no await).
        let json = to_json("echo hello world");
        assert!(json.contains("\"type\":\"Program\""));
        assert!(json.contains("\"name\":\"write\""));
        assert!(json.contains("\"name\":\"process\""));
        assert!(json.contains("hello"));
        assert!(!json.contains("\"name\":\"builtin\""));
        assert!(!json.contains("\"name\":\"exec\""));
        assert!(!json.contains("unsupported"));
        // a GLOB arg keeps the runtime builtin (it glob-expands)
        let json2 = to_json("echo *.txt");
        assert!(json2.contains("\"name\":\"builtin\""));
        assert!(!json2.contains("\"name\":\"write\""));
    }

    #[test]
    fn echo_single_arg_skips_the_join() {
        // `echo "$i"` — one QUOTED non-literal arg: `[String(i)].join(" ")` is
        // exactly `String(i)` (a one-element join never inserts the
        // separator), so the emitter emits the bare value — no array /
        // join machinery. An UNQUOTED `echo $i` is a field-split arg (the
        // A1 split marker); when the split is provably a no-op (a
        // numeric/nospace value — see expr_known_nospace) the arg is a
        // single provably-scalar value and unwraps to the bare binding
        // too. An un-scalarizable split (unknown/multi-word value) or an
        // array-valued arg keeps the flat/join path (the shortcut would
        // comma-join a multi-word value).
        let json = to_json("i=42; echo \"$i\"");
        assert!(json.contains("\"name\":\"String\""));
        assert!(!json.contains("\"name\":\"join\""), "single arg: no join");
        assert!(
            !json.contains("\"type\":\"ArrayExpression\""),
            "single arg: no array"
        );
        assert!(!json.contains("unsupported"));
        // unquoted but numeric: the field-split is a provable no-op (i is
        // a numeric var) — the single scalar arg unwraps, no flat/join
        let json_unq = to_json("i=42; echo $i");
        assert!(!json_unq.contains("\"name\":\"join\""), "numeric single arg: no join");
        assert!(
            !json_unq.contains("\"type\":\"ArrayExpression\""),
            "numeric single arg: no array"
        );
        // two args keep the word-join
        let json2 = to_json("i=42; echo $i $i");
        assert!(json2.contains("\"name\":\"join\""));
        assert!(json2.contains("\"type\":\"ArrayExpression\""));
        assert!(!json2.contains("unsupported"));
        // an ARRAY-VALUED single arg (unquoted `$(...)` captureWords — the
        // runtime splices its words) must still splice + join — it is not
        // a scalar; a capture ASSIGNED to a var is a scalar string
        let json3 = to_json("echo $(echo 1 2 3)");
        assert!(json3.contains("\"name\":\"join\""));
        assert!(json3.contains("\"name\":\"flat\""));
        assert!(!json3.contains("unsupported"));
        let json4 = to_json("x=$(echo 1 2 3); echo \"$x\"");
        assert!(
            !json4.contains("\"name\":\"join\""),
            "capture-assigned var is a scalar"
        );
        assert!(!json4.contains("unsupported"));
    }

    #[test]
    fn join_of_string_slice_chain_is_identity() {
        // `${name:0:4}` — the param-slice lowering emits
        // `String(name).slice(0, 4)` and the interpolation joins it. The
        // join of a provably-STRING value is identity (the runtime join is
        // `Array.isArray(v) ? v.join(" ") : String(v)`), so the runtime
        // call must disappear even when the chain carries call args
        // (`String(name).slice(0, 4)` is a CallExpression whose callee is
        // the `.slice` member — the old root-only scan missed it and left
        // 5 corpus sites on the runtime join).
        let json = to_json("n=hello; echo \"${n:0:4}\"");
        assert!(json.contains("\"name\":\"slice\""));
        assert!(!json.contains("\"name\":\"join\""), "no runtime join");
        assert!(!json.contains("unsupported"));
    }

    #[test]
    fn set_double_dash_assigns_positionals_natively() {
        // `set -- a b c d` — the `--` marker ends the option list: the
        // remaining args are the POSITIONALS. The flag path must not
        // swallow them (its `try_native_set_flags` treats `--` as a flag
        // with no letters and would emit only `(lastExit = 0, true)` —
        // the positional write lost — parse-at-slice.sh printed ""
        // instead of "c d").
        let json = to_json("set -- a b c d; echo \"${@:3}\"");
        assert!(json.contains("\"property\":{\"type\":\"Identifier\",\"name\":\"positional\"")
            || json.contains("\"name\":\"positional\""));
        assert!(
            json.contains("\"name\":\"set\"") || json.contains("\"value\":\"a\""),
            "the positionals a b c d are assigned"
        );
        assert!(!json.contains("unsupported"));
    }

    #[test]
    fn capture_words_single_word_wc_count_is_native() {
        // `local size=$(wc -c < f)` — the UNQUOTED capture form: the
        // runtime splits the capture on IFS whitespace. The wc count is
        // provably a single word (digits), so the machinery collapses to
        // a native readFile + byte-count inside the promise's success
        // branch (a failed redirect yields "" — the count must not apply
        // to the error sentinel, and an empty file must still count 0).
        let json = to_json(
            "f() { local size=$(wc -c < /etc/hostname); echo \"size=$size\"; }; f",
        );
        assert!(
            !json.contains("\"name\":\"captureWords\""),
            "no captureWords machinery"
        );
        assert!(json.contains("\"name\":\"readFile\""));
        assert!(json.contains("\"name\":\"then\""), "the count maps inside the promise");
        assert!(!json.contains("\"value\":\"wc\""), "no wc dispatch");
        assert!(!json.contains("unsupported"));
    }

    #[test]
    fn grep_null_test_lifts_to_contains() {
        // `if echo $x | grep P >/dev/null 2>/dev/null` (discarded-output grep
        // as a test) is a substring test — no echo/grep spawns, no pipeline;
        // the emitter inlines the ShIR `contains` call to a NATIVE
        // `String(h).includes(n)` (src/shir.rs expr_to_estree).
        let json = to_json("if echo hi | grep hi > /dev/null 2> /dev/null; then echo yes; fi");
        assert!(json.contains("\"name\":\"includes\""));
        assert!(json.contains("\"name\":\"String\""));
        assert!(!json.contains("pipeline"));
        assert!(!json.contains("\"name\":\"grep\""));
        assert!(!json.contains("unsupported"));
    }

    #[test]
    fn echo_pipe_cut_lifts_to_native_slice() {
        // `$(echo X | cut -c3-)` — the echo|cut capture: a pure string-op
        // chain (split/map/slice/join) — no pipeline, no capture, no
        // builtin dispatch.
        let json = to_json("x=$(echo hi | cut -c3-)");
        assert!(json.contains("\"name\":\"slice\""));
        assert!(json.contains("\"name\":\"map\""));
        assert!(!json.contains("\"name\":\"pipeline\""));
        assert!(!json.contains("\"name\":\"capture\""));
        assert!(!json.contains("\"name\":\"cut\""));
        assert!(!json.contains("unsupported"));
        // -d/-f: a field pick — split/filter/join over the echoed text
        let json2 = to_json("x=$(echo a:b:c | cut -d: -f2)");
        assert!(json2.contains("\"name\":\"filter\""));
        assert!(json2.contains("\"name\":\"includes\""));
        assert!(!json2.contains("\"name\":\"pipeline\""));
        assert!(!json2.contains("\"name\":\"cut\""));
        assert!(!json2.contains("unsupported"));
    }

    #[test]
    fn echo_pipe_cut_statement_uses_cuttext() {
        // statement-form `echo X | cut OP` → the sync cutText helper
        // (the grepText precedent) — no async pipeline machinery
        let json = to_json("echo a:b | cut -d: -f1");
        assert!(json.contains("\"name\":\"cutText\""));
        assert!(!json.contains("\"name\":\"pipeline\""));
        assert!(!json.contains("unsupported"));
    }

    #[test]
    fn bare_env_lowers_to_sync_builtin() {
        // `env | grep '^myexport='` — the bare env form (no operands):
        // the subprocess spawn collapses to the sync builtin dispatch
        // (builtins.env dumps process.env — sink-correct everywhere). A
        // flag/carrying form (`env -i`) keeps the exec spawn (the
        // builtin cannot run commands).
        let json = to_json("env | grep '^myexport='");
        assert!(json.contains("\"value\":\"env\""));
        assert!(json.contains("\"name\":\"builtin\""));
        assert!(!json.contains("\"name\":\"exec\""), "no env spawn");
        assert!(!json.contains("unsupported"));
        let json2 = to_json("env -i foo");
        assert!(json2.contains("\"name\":\"exec\""), "flag forms keep the spawn");
        assert!(!json2.contains("unsupported"));
    }

    #[test]
    fn egrep_lowers_to_sync_builtin() {
        // `$(egrep PAT FILE)` — GNU's grep -E alias: SYNC_BUILTINS admits
        // the name, so the exec spawn becomes the sync builtin dispatch
        // (builtins.egrep = grep with -E prepended).
        let json = to_json("x=$(egrep '^pattern' /dev/null)");
        assert!(json.contains("\"value\":\"egrep\""));
        assert!(json.contains("\"name\":\"builtin\""));
        assert!(!json.contains("\"name\":\"exec\""), "no egrep spawn");
        assert!(!json.contains("unsupported"));
    }

    #[test]
    fn echo_pipe_bc_statement_folds_to_native_write() {
        // statement-form `echo "2+3" | bc` with a STATIC program → the
        // compile-time bc fold (src/bc.rs): the pipeline + bc subprocess
        // spawn collapse to a native `process.stdout.write("5\n")` +
        // status sequence (the try_native_echo_bc_stmt twin of the
        // capture-position fold). A DYNAMIC program (`$x + 1` — no
        // runtime bc evaluator) keeps the spawn.
        let json = to_json("echo \"2+3\" | bc");
        assert!(json.contains("\"name\":\"write\""));
        assert!(json.contains("\"value\":\"5\\n\""), "folded 2+3 -> 5\n");
        assert!(!json.contains("\"name\":\"pipeline\""));
        assert!(!json.contains("\"name\":\"exec\""));
        assert!(!json.contains("unsupported"));
        // multi-statement programs fold too (scale=2; 5/2 -> 2.50)
        let json2 = to_json("echo \"scale=2; 5/2\" | bc");
        assert!(json2.contains("\"value\":\"2.50\\n\""));
        assert!(!json2.contains("\"name\":\"exec\""));
        // a dynamic program keeps the pipeline + bc spawn
        let json3 = to_json("x=1; echo \"$x + 1\" | bc");
        assert!(json3.contains("\"name\":\"pipeline\""));
        assert!(json3.contains("\"name\":\"exec\""));
        assert!(!json3.contains("unsupported"));
    }

    #[test]
    fn cut_herestring_capture_lifts_to_native() {
        // `$(cut -c2 <<< X)` — the here-string feed is the same per-line
        // selection over the target value; the split has no trailing ''
        // (bash appends the newline, the runtime pops it)
        let json = to_json("x=$(cut -c2 <<< hi)");
        assert!(json.contains("\"name\":\"slice\""));
        assert!(!json.contains("\"name\":\"capture\""));
        assert!(!json.contains("\"name\":\"redirect\""));
        assert!(!json.contains("\"name\":\"cut\""));
        assert!(!json.contains("unsupported"));
    }

    #[test]
    fn cut_dynamic_args_not_lifted() {
        // a dynamic cut arg (a variable position list) keeps the runtime
        // pipeline + builtin — the sync twin (both stages are sync)
        let json = to_json("x=$(echo a:b:c | cut -d: -f$n)");
        assert!(json.contains("\"name\":\"pipelineSync\""));
        assert!(!json.contains("\"name\":\"slice\""));
        assert!(!json.contains("\"name\":\"filter\""));
        assert!(!json.contains("unsupported"));
    }

    #[test]
    #[test]
    fn sleep_lowers_to_native_timer() {
        // `sleep 1` — the exec spawn collapses to a native async timer
        // `(await new Promise(r => setTimeout(() => r(true), 1000)), …)`:
        // no exec dispatch, no subprocess. A fractional literal folds to
        // ms too (`sleep 0.1` → 100); a dynamic arg becomes
        // `Number(<value>) * 1000` (the runtime's arg flattener turns the
        // unquoted-expansion split array into one arg for a single-word
        // value).
        let json = to_json("sleep 1");
        assert!(json.contains("\"type\":\"NewExpression\""));
        assert!(json.contains("setTimeout"));
        assert!(json.contains("1000"));
        assert!(!json.contains("\"name\":\"exec\""));
        assert!(!json.contains("unsupported"));
        let json2 = to_json("sleep 0.1");
        assert!(json2.contains("100"), "fractional seconds fold to ms");
        assert!(!json2.contains("\"name\":\"exec\""));
        let json3 = to_json("sleep $n");
        assert!(json3.contains("Number"));
        assert!(!json3.contains("\"name\":\"exec\""));
        // a command named sleep with extra args keeps the runtime (bash
        // would error — the spawn path reports it)
        let json4 = to_json("sleep 1 2");
        assert!(json4.contains("\"name\":\"exec\""));
        // the env-carrying form keeps the runtime (command-scoped env)
        let json5 = to_json("TZ=UTC sleep 1");
        assert!(json5.contains("\"name\":\"exec\""));
    }

    #[test]
    fn dollar_ref_arith_text_lowers_natively() {
        // `j=$(( $j*$i ))` INSIDE a for loop (047_for_arithematic): the
        // runtime arith STRING's `$name` refs strip to a native-lowerable
        // expression, so the loop body must become bare native JS — no
        // setVar, no sh2.arith. The refs are PROVABLY SET here (i is the
        // loop var, j was assigned 0 before the loop... the deletion gate
        // keeps the unset-j shape on the runtime, see below).
        let json = to_json("i=0\nj=0\nj=$(( $j*$i ))");
        assert!(
            !json.contains("\"name\":\"arith\""),
            "native-lowerable $ref arith text: no sh2.arith"
        );
        assert!(!json.contains("unsupported"));
        // the UNSET gate: `j=$(( $j*$i ))` with j's ONLY write the arith
        // text itself — bash substitutes the EMPTY value, `*i` is a
        // syntax error, the assignment is skipped (047 in the corpus).
        // The deletion gate must keep the runtime evaluator.
        let json2 = to_json("for i in 1 2 3; do j=$(( $j*$i )); done");
        assert!(
            json2.contains("\"name\":\"arith\""),
            "unset-at-read $ref arith text keeps sh2.arith"
        );
        assert!(!json2.contains("unsupported"));
        // `let "$n == 5"` — the `(( $n == 5 ))` condition: n is set, the
        // stripped text parses — the let must lower natively.
        let json3 = to_json("n=5\nif (( $n == 5 )); then echo equal; fi");
        assert!(
            !json3.contains("\"name\":\"builtin\""),
            "$ref let cond: no builtin dispatch"
        );
        assert!(!json3.contains("unsupported"));
        // `$1` positionals are NOT strip-able — the runtime stays.
        let json4 = to_json("n=$(( $1 + 0 ))");
        assert!(json4.contains("\"name\":\"arith\""));
        assert!(!json4.contains("unsupported"));
    }

    #[test]
    fn yes_head_capture_lifts_to_native_repeat() {
        // `$(yes Hello | head -3)` — the infinite-producer capture: yes
        // prints `Hello\n` forever, head takes the first 3 lines — the
        // captured value is exactly `(Hello + "\n").repeat(3)` with the
        // capture strips: no pipeline, no capture arrow, no spawns.
        let json = to_json("x=$(yes Hello | head -3)");
        assert!(json.contains("\"name\":\"repeat\""));
        assert!(json.contains("Hello"));
        assert!(!json.contains("\"name\":\"pipeline\""));
        assert!(!json.contains("\"name\":\"capture\""));
        assert!(!json.contains("\"name\":\"yes\""));
        assert!(!json.contains("unsupported"));
        // `head -n 3` / `head -n3` forms lift too; a dynamic head count
        // keeps the runtime pipeline.
        let json2 = to_json("x=$(yes Hi | head -n 3)");
        assert!(json2.contains("\"name\":\"repeat\""));
        let json3 = to_json("x=$(yes Hi | head -n $n)");
        assert!(json3.contains("\"name\":\"pipeline\""));
        assert!(!json3.contains("\"name\":\"repeat\""));
    }

    #[test]
    fn ls_lowers_to_sync_builtin_and_hostname_capture_is_native() {
        // `ls` is a native sync builtin (the GNU-faithful listing): the
        // exec dispatch lowers to the sync twin — no spawn.
        let json = to_json("ls -A");
        assert!(json.contains("\"name\":\"builtin\""));
        assert!(!json.contains("\"name\":\"exec\""));
        assert!(!json.contains("unsupported"));
        // `$(hostname)` — the value-returning runtime twin (like uname/
        // date/readlink): no capture machinery, no spawn.
        let json2 = to_json("h=$(hostname)");
        assert!(json2.contains("\"name\":\"hostname\""));
        assert!(!json2.contains("\"name\":\"capture\""));
        assert!(!json2.contains("\"name\":\"exec\""));
        assert!(!json2.contains("unsupported"));
    }

    #[test]
    fn substitute_all_uses_replace_all_for_dollar_free_replacement() {
        // `${x//p/r}` with a literal pattern — the runtime's literal fast
        // path is split/join; a `$`-free replacement lowers one step
        // further to String.replaceAll (single-pass, same literal
        // semantics). A `$`-bearing replacement keeps split/join (JS
        // replaceAll would treat `$&`/`$1` as substitution sequences).
        let json = to_json("echo \"${x//o/0}\"");
        assert!(json.contains("\"name\":\"replaceAll\""));
        assert!(!json.contains("unsupported"));
        // a `$`-bearing replacement keeps the RUNTIME param call (JS
        // replaceAll would treat `$&`/`$1` as substitution sequences, and
        // the positional default is not fully liftable anyway)
        let json2 = to_json(r#"echo "${x//o/$1}""#);
        assert!(json2.contains("\"name\":\"param\""));
        assert!(!json2.contains("\"name\":\"replaceAll\""));
    }

    #[test]
    fn never_written_var_reads_fold_to_empty() {
        // SH2_ASSUME_NO_ENV fold: a name with NO write anywhere in the
        // program reads as the constant "" (the runtime would return the
        // env fallback, which the documented assumption declares
        // unobservable). `x`/`y` are never written — `echo "$x $y"`
        // lowers without a single getVar.
        let json = to_json("echo \"$x $y\"");
        assert!(!json.contains("\"getVar\""));
        // the read-builtin vars are writes: `read x` marks x — the read
        // stays LIVE, but as the native plain-object store read (the
        // runtime read's setVar write is the plain path — no dispatch)
        let json2 = to_json("read x; echo \"$x\"");
        assert!(json2.contains("\"name\":\"vars\""));
        assert!(!json2.contains("\"getVar\""));
        // an eval/source program disables the fold entirely (the eval
        // may write the name at runtime — the read must stay LIVE): the
        // native store read `sh2.vars.x ?? env ?? ''` (the runtime's
        // exact plain path — a getVar CALL would be a dispatch)
        let json3 = to_json("eval \"echo hi\"; echo \"$x\"");
        assert!(json3.contains("\"name\":\"vars\""));
        assert!(json3.contains("\"name\":\"x\""));
        // a nameref TARGET is a write: `typeset -n r=x` makes `r=5`
        // write x through the runtime's refVars indirection
        let json4 = to_json("typeset -n r=x; r=5; echo \"$x\"");
        assert!(json4.contains("\"name\":\"vars\""));
        // a runtime `let` writes its arith idents: `let var++` (the JS
        // keyword keeps the var store-bound) must not fold the read —
        // the native store read (a `vars.var` property access is legal
        // JS even for keyword names)
        let json5 = to_json("let var++; echo \"$var\"");
        assert!(json5.contains("\"name\":\"vars\""));
    }

    #[test]
    fn and_or_chain_store_var_tests_lower_native() {
        // `[[ "$a" == "x" ]] && [[ "$b" == "y" ]]` — the chain links
        // branch on lastExit, so each test records its status natively
        // (`(sh2._g = String(sh2.getVar(a)) === "x", sh2.lastExit =
        // sh2._g ? 0 : 1, sh2._g)` — the `_g` scratch evaluates the read
        // EXACTLY ONCE, keeping the call-site count at ONE getVar vs the
        // single runtime test it replaces). No sh2.test dispatch, no
        // string tokenize/parse per evaluation.
        let json = to_json("if [[ \"$a\" == \"x\" ]] && [[ \"$b\" == \"y\" ]]; then echo yes; fi");
        // the sh2.test DISPATCH is gone (a regex-literal `.test()` method
        // call has a different callee shape and may legitimately appear)
        assert!(!json
            .contains("\"name\":\"sh2\"},\"property\":{\"type\":\"Identifier\",\"name\":\"test\""));
        // `$a`/`$b` are NEVER WRITTEN — the SH2_ASSUME_NO_ENV fold
        // lowers their reads to the constant "" (no getVar at all); the
        // `_g` scratch still evaluates the single read exactly once
        assert!(!json.contains("\"name\":\"getVar\""));
        assert!(json.contains("\"name\":\"_g\""));
        assert!(!json.contains("unsupported"));
    }

    #[test]
    fn compound_test_lowers_to_native_or() {
        // `[[ "$2" == "test" || "$2" == "debug" ]]` — the test-level
        // `-o` compound: each leaf lowers (positional reads — ZERO
        // dispatches) and the leaves join with a native `||` — the
        // runtime test call (tokenize + parse + dispatch) disappears.
        let json = to_json(
            "if [[ \"$1\" =~ ^[0-9]+$ ]] && [[ \"$2\" == \"test\" || \"$2\" == \"debug\" ]]; then echo ok; fi",
        );
        assert!(!json
            .contains("\"name\":\"sh2\"},\"property\":{\"type\":\"Identifier\",\"name\":\"test\""));
        assert!(json.contains("\"name\":\"positional\""));
        assert!(json.contains("\"operator\":\"||\""));
        assert!(!json.contains("unsupported"));
    }

    #[test]
    fn test_lowerings_extglob_and_quoted_spaces_and_lt() {
        // The test-expression lowering family:
        // 1. extglob `==` (`[[ $x == !(*.min).js ]]` — bash matches the
        //    pattern with extglob semantics) → an anchored regex literal
        //    with the `s` flag (dotAll — the runtime's `*`/`?` match any
        //    char incl. newlines): `^[\\s\\S]*(?<!(?:\\.min))\\.js$`.
        // 2. `[[ "a" < "b" ]]` lexical `<` → a native JS string `<`.
        // 3. `[[ "hello world" =~ ^hello ]]` — a quoted literal WITH a
        //    space in the `=~` value operand → native regex `.test`.
        // 4. `[[ ! -e /no/such/file ]]` — the `!` without a space before
        //    the file-test flag → `!sh2.fileTest(...)`.
        // 5. `[ "$(echo hello)" = "hello" ]` — a literal echo cmdsub
        //    operand → the compile-time folded value.
        let json = to_json(
            "shopt -s extglob; f=file.js; [[ $f == !(*.min).js ]] && echo a; [[ \"a\" < \"b\" ]] && echo b; [[ \"hello world\" =~ ^hello ]] && echo c; [[ ! -e /no/such/file ]] && echo d; [ \"$(echo hello)\" = \"hello\" ] && echo e",
        );
        // no sh2.test DISPATCH anywhere (a regex-literal `.test()` method
        // call has a different callee shape)
        assert!(!json
            .contains("\"name\":\"sh2\"},\"property\":{\"type\":\"Identifier\",\"name\":\"test\""));
        // the extglob lookbehind regex
        assert!(json.contains("(?<!"));
        assert!(json.contains("\\\\.min"));
        // the `s` flag on the regex literal
        assert!(json.contains("\"flags\":\"s\""));
        // the lexical `<`
        assert!(json.contains("\"operator\":\"<\""));
        // the quoted-space `=~` literal
        assert!(json.contains("hello world"));
        // the `!`-file-test
        assert!(json.contains("\"name\":\"fileTest\""));
        assert!(json.contains("\"operator\":\"!\""));
        // the folded echo cmdsub literal
        assert!(json.contains("\"value\":\"hello\""));
        assert!(!json.contains("unsupported"));
    }

    #[test]
    fn dynamic_value_local_decl_lifts_to_let() {
        // The `declare_sources_dyn` widening: `local x=<dynamic value>`
        // whose value the runtime builtin receives pre-evaluated lifts to
        // a native `let` — no sh2.builtin("local") dispatch, no store
        // round-trip. Shapes: `$(wc -c < f)` (single-word capture — the
        // one-element array unwraps to the word), `$(echo a b c)`
        // (multi-word capture — the RAW capture text, bash does not
        // word-split in assignment context), `${2:-d}` param ops,
        // `$((x+y))` arith (String-wrapped — the store's string model),
        // `$?` (the lastExit read) and dynamic interpolates.
        let json = to_json(
            "f() { local sz=$(wc -c < \"$f\"); local mw=$(echo a b c); local p=\"${2:-d}\"; local ar=$((x + y)); local ec=$?; local z=\"lit $y\"; echo \"$sz $mw $p $ar $ec $z\"; }; f",
        );
        // NO builtin LOCAL dispatch (the echo inside the function stays a
        // sink-bound builtin — that is not this family)
        assert!(!json.contains("\"value\":\"local\""));
        // the single-word wc capture unwraps to the native value
        assert!(json.contains("\"name\":\"size\"") || json.contains("\"name\":\"sz\""));
        // the multi-word capture folds to the raw text (the capture
        // twin's echo fold — "a b c", the no-split assignment value)
        assert!(json.contains("\"value\":\"a b c\""));
        // the `$?` value is the native lastExit read
        assert!(json.contains("\"name\":\"lastExit\""));
        // the arith value is String-wrapped for the string binding
        assert!(json.contains("\"name\":\"String\""));
        // the dynamic interpolate is a template literal value
        assert!(json.contains("\"type\":\"TemplateLiteral\""));
        assert!(!json.contains("unsupported"));
    }

    #[test]
    fn nocase_dynamic_literal_test_folds_when_invariant() {
        // `shopt -s nocasematch` + `shopt -u nocasematch` both present →
        // the shopt state is DYNAMIC, so runtime-dependent comparisons
        // stay on the runtime test call — EXCEPT literal-vs-literal
        // comparisons whose result is case-folding-invariant (`"abc" ==
        // "abc"` is true under every state and folds natively; `"ABC"
        // == "abc"` differs by state and must stay on the runtime).
        let json = to_json(
            "shopt -s nocasematch; [[ \"abc\" == \"abc\" ]] && echo a; [[ \"ABC\" == \"abc\" ]] && echo b; shopt -u nocasematch",
        );
        // the invariant comparison folded natively (the sh2.test DISPATCH
        // that remains belongs to the `"ABC" == "abc"` case)
        assert!(json.contains("\"value\":\"abc\""));
        assert!(!json.contains("unsupported"));
        // and the state-dependent one keeps the runtime call
        let json2 = to_json(
            "shopt -s nocasematch; [[ \"ABC\" == \"abc\" ]] && echo b; shopt -u nocasematch",
        );
        assert!(json2
            .contains("\"name\":\"sh2\"},\"property\":{\"type\":\"Identifier\",\"name\":\"test\""));
    }

    #[test]
    fn status_equality_lowers_to_lastexit_read() {
        // `[ "$?" = "0" ]` — the `$?` sigil is a status-field read, not
        // a glob `?`: `String(sh2.lastExit) === "0"`, zero dispatches.
        let json = to_json("if [ \"$?\" = \"0\" ]; then echo zero; fi");
        assert!(!json
            .contains("\"name\":\"sh2\"},\"property\":{\"type\":\"Identifier\",\"name\":\"test\""));
        assert!(json.contains("\"name\":\"lastExit\""));
        assert!(json.contains("\"name\":\"String\""));
        assert!(!json.contains("unsupported"));
    }

    #[test]
    fn grep_with_regex_pattern_not_lifted() {
        // BRE metacharacters disqualify the lift: `grep 'a.c'` is a regex,
        // not a substring test — the pipeline must stay.
        let json = to_json("if echo hi | grep a.c > /dev/null 2> /dev/null; then echo yes; fi");
        assert!(!json.contains("contains"));
        assert!(json.contains("pipeline"));
    }

    #[test]
    fn statement_pipeline_grep_not_lifted() {
        // Statement-position `echo x | grep y >/dev/null` keeps its $? status
        // (read back by the next command) — only test-position conds lift.
        let json = to_json("echo hi | grep hi > /dev/null 2> /dev/null; echo $?");
        assert!(!json.contains("contains"));
        assert!(json.contains("pipeline"));
    }

    #[test]
    fn batch_ok_glob_for_lowers_to_forLoopBatch() {
        // The sync-ok-loops transform's `batch_ok` verdict (the core
        // request estree-20260805-045731): a top-level for loop whose body
        // is sync-executable but whose GLOB iterable disqualifies the
        // native for-of (the runtime must glob-expand) emits the
        // checkpointed `await sh2.forLoopBatch(iter, body, 1024)` instead
        // of the blocking `forLoopSync` — sync chunks of 1024 with a
        // setImmediate yield, same flatten/glob/signal semantics.
        let json = to_json("for f in *.sh; do echo \"$f\"; done");
        assert!(json.contains("\"name\":\"forLoopBatch\""));
        assert!(json.contains("\"value\":1024"));
        assert!(json.contains("\"type\":\"AwaitExpression\""));
        assert!(!json.contains("\"name\":\"forLoopSync\""));
        assert!(!json.contains("\"type\":\"ForOfStatement\""));
        // a PLAIN (glob-free) iterable keeps the native for-of — no batch
        let json2 = to_json("for i in a b c; do echo $i; done");
        assert!(json2.contains("\"type\":\"ForOfStatement\""));
        assert!(!json2.contains("\"name\":\"forLoopBatch\""));
        // a batch_ok loop whose body's capture is now SYNC (the *Sync
        // family: `$(ls)` is a sync builtin → captureSync, no await) has
        // an await-free body → the loop lifts to the NATIVE for-of (the
        // best rung) — the old "awaiting body" premise only holds for
        // genuinely async captures (spawns)
        let json3 = to_json("for i in 1 2 3; do x=$(ls); done");
        assert!(!json3.contains("\"name\":\"forLoopBatch\""));
        assert!(!json3.contains("\"name\":\"forLoopSync\""));
        assert!(json3.contains("\"type\":\"ForOfStatement\""));
        assert!(json3.contains("\"name\":\"captureSync\""));
        assert!(!json3.contains("\"name\":\"forLoop\""));
        assert!(!json3.contains("unsupported"));
        // a genuinely async capture (a spawn) keeps the async forLoop
        let json5 = to_json("for i in 1 2 3; do x=$(awk '{print $1}'); done");
        assert!(json5.contains("\"type\":\"AwaitExpression\""));
        assert!(json5.contains("\"name\":\"forLoop\""));
    }

    #[test]
    fn seq_range_for_lowers_to_native_for_statement() {
        // `for i in $(seq 1 10000)` — the seq_range_for transform
        // rewrites the captureWords iterable to a Range, and the emitter
        // lowers it to a native JS counter loop
        // (`for (let i = 1; i <= 10000; i++)`) — the hand-written ideal.
        // No capture, no runtime loop call, no item list, no per-iteration
        // coercion.
        let json = to_json("for i in $(seq 1 3); do echo $i; done");
        assert!(json.contains("\"type\":\"ForStatement\""));
        assert!(json.contains("\"operator\":\"<=\""));
        assert!(json.contains("\"operator\":\"++\""));
        assert!(!json.contains("\"name\":\"captureWords\""));
        assert!(!json.contains("\"name\":\"forLoop\""));
        assert!(!json.contains("\"type\":\"ForOfStatement\""));
        assert!(!json.contains("unsupported"));
        // the sqrt1337 shape: the grep test lifts to String(...).includes
        // AND the loop var lifts to a native number — `i * i`, no
        // `(Number(i) || 0)` coercion
        let json2 = to_json(
            "for i in $(seq 1 10000); do if echo $((i*i)) | grep 1337 >/dev/null 2>/dev/null; then echo $i; fi; done",
        );
        assert!(json2.contains("\"type\":\"ForStatement\""));
        assert!(json2.contains("\"name\":\"includes\""));
        assert!(json2.contains("\"operator\":\"*\""));
        assert!(
            !json2.contains("\"name\":\"Number\""),
            "no (Number(i) || 0) coercion"
        );
        assert!(!json2.contains("\"name\":\"captureWords\""));
        assert!(!json2.contains("unsupported"));
    }

    #[test]
    fn seq_range_for_conservative_cases() {
        // 3-arg step forms (`seq A S B`) keep the runtime path — the
        // capture is sync now (seq is a sync builtin → captureWordsSync)
        let json = to_json("for i in $(seq 1 2 10); do echo $i; done");
        assert!(json.contains("\"name\":\"captureWordsSync\""));
        assert!(!json.contains("\"type\":\"ForStatement\""));
        // leading-zero args (`seq 01 10` — GNU pads, bash arith is octal)
        let json2 = to_json("for i in $(seq 01 10); do echo $i; done");
        assert!(json2.contains("\"name\":\"captureWordsSync\""));
        assert!(!json2.contains("\"type\":\"ForStatement\""));
        // a body WRITE to the loop var keeps word-list semantics (a
        // counter's i++ would read the body-written value)
        let json3 = to_json("for i in $(seq 1 3); do i=99; echo $i; done");
        assert!(json3.contains("\"name\":\"captureWordsSync\""));
        assert!(!json3.contains("\"type\":\"ForStatement\""));
        // a nested loop binding the SAME var keeps the OUTER on the word
        // path (bash clobbers i in the body; a counter's i++ would read
        // the body-written value). The INNER loop — whose own body never
        // writes its var — still transforms independently.
        let json4 =
            to_json("for i in $(seq 1 2); do for i in $(seq 10 12); do echo $i; done; done");
        assert!(
            json4.contains("\"name\":\"captureWordsSync\""),
            "outer keeps the word list"
        );
        assert!(
            json4.contains("\"type\":\"ForOfStatement\""),
            "outer is a word loop"
        );
        assert!(
            json4.contains("\"type\":\"ForStatement\""),
            "inner is a counter loop"
        );
    }

    #[test]
    fn seq_range_for_post_loop_read_stores_last_value() {
        // the loop var read AFTER the loop stays store-backed — the
        // store-sync elimination emits a pre-loop getVar into a temp, the
        // per-iteration temp write, and a post-loop setVar of the LAST
        // value (bash leaves $i = 10000). Since the store-to-native
        // transform (core request shir-passes-store-to-native-20260806)
        // the post-loop sync is a NATIVE write `i = __sh2_for_last_i` —
        // the lifted binding IS the post-loop read target, so the store
        // round-trip is pure overhead (the runtime would read the same
        // binding back).
        let json = to_json("for i in $(seq 1 3); do echo $i; done; echo $i");
        assert!(json.contains("\"type\":\"ForStatement\""));
        assert!(json.contains("\"name\":\"__sh2_for_last_i\""));
        assert!(json.contains("\"type\":\"AssignmentExpression\""));
        assert!(!json.contains("\"name\":\"setVar\""));
        assert!(!json.contains("\"name\":\"captureWords\""));
        assert!(!json.contains("unsupported"));
    }

    #[test]
    fn sync_ok_capture_loop_stays_native_for_of() {
        // A cheap capture loop (`{1..1000}`, ~3ms ≤ the 200ms budget) is
        // sync_ok: the existing sync gate emits the native for-of inside
        // the capture arrow — no runtime loop call at all.
        let json = to_json("x=$(for i in {1..1000}; do echo $i; done)\necho ${x:0:1}");
        assert!(json.contains("\"type\":\"ForOfStatement\""));
        assert!(!json.contains("\"name\":\"forLoop\""));
        assert!(!json.contains("\"name\":\"forLoopSync\""));
        assert!(!json.contains("\"name\":\"forLoopBatch\""));
        assert!(!json.contains("unsupported"));
    }

    #[test]
    fn assignment_lowers_to_setvar() {
        // provably-numeric/string variables are LIFTED to native JS writes
        // (`x = 42` / `x = \"hello\"`), no runtime store round-trip
        let json = to_json("x=42");
        assert!(json.contains("\"type\":\"AssignmentExpression\""));
        assert!(!json.contains("\"name\":\"setVar\""));
        assert!(!json.contains("unsupported"));
        let json2 = to_json("x=hello");
        assert!(json2.contains("\"type\":\"AssignmentExpression\""));
        assert!(!json2.contains("\"name\":\"setVar\""));
        // a capture source LIFTS too: `x=$(cmd)` is a native assignment of
        // the capture value (the runtime capture always yields a string) —
        // no store round-trip. A pure echo capture lowers all the way to a
        // native join; other captures keep the await sh2.capture call.
        let json3 = to_json("x=$(echo hi)");
        assert!(json3.contains("\"type\":\"AssignmentExpression\""));
        // the pure echo capture folds to a single literal (the join of one
        // literal arg is the literal itself — no runtime join machinery)
        assert!(json3.contains("\"value\":\"hi\""));
        assert!(!json3.contains("\"name\":\"join\""));
        assert!(!json3.contains("\"name\":\"setVar\""));
        assert!(!json3.contains("unsupported"));
        let json4 = to_json("x=$(date)");
        assert!(json4.contains("\"type\":\"AssignmentExpression\""));
        // `$(date)` lifts to the native value twin of the sync builtin
        // (sh2.date — no capture machinery, no spawn); a genuinely
        // external capture keeps the await sh2.capture call.
        assert!(json4.contains("\"name\":\"date\""));
        assert!(!json4.contains("\"name\":\"capture\""));
        assert!(!json4.contains("\"name\":\"setVar\""));
    }

    #[test]
    fn if_then_else_lowers_to_if_statement() {
        // `[ -f /tmp/x ]` is a file test — a sync `sh2.fileTest(flag,
        // path)` runtime call (evalUnary minus the string parse/dispatch;
        // no async lstat chain — the chain was the last await in
        // otherwise-sync loop bodies), wrapped in the native-test status
        // protocol (`sh2._g = ...`, `sh2.lastExit = ...`). No sh2.test
        // string parse, no dispatch.
        let json = to_json("if [ -f /tmp/x ]; then echo yes; else echo no; fi");
        assert!(json.contains("\"type\":\"IfStatement\""));
        assert!(json.contains("\"name\":\"fileTest\""));
        assert!(!json.contains("\"name\":\"lstat\""));
        assert!(!json.contains("\"name\":\"test\""));
        assert!(!json.contains("unsupported"));
    }

    #[test]
    fn variable_and_interpolation() {
        // `name` is string-lifted: the template interpolates the native var
        let json = to_json("name=world\necho \"Hello $name\"");
        assert!(json.contains("\"type\":\"TemplateLiteral\""));
        assert!(json.contains("\"type\":\"Identifier\""));
        assert!(!json.contains("\"name\":\"getVar\""));
        assert!(!json.contains("unsupported"));
        // a CAPTURE source lifts to a native binding; a read/write-builtin
        // var (read/declare/local/export...) stays a store read. The echo
        // has ONE interpolated arg — the single-arg collapse emits the
        // bare template, no join.
        let json2 = to_json("name=$(echo world)\necho \"Hello $name\"");
        assert!(!json2.contains("\"name\":\"getVar\""));
        assert!(
            !json2.contains("\"name\":\"join\""),
            "single interpolated arg: no join"
        );
        assert!(json2.contains("\"type\":\"TemplateLiteral\""));
        assert!(!json2.contains("unsupported"));
        let json3 = to_json("read name\necho \"Hello $name\"");
        assert!(json3.contains("\"name\":\"vars\""));
        assert!(!json3.contains("\"name\":\"getVar\""));
        assert!(!json3.contains("unsupported"));
    }

    #[test]
    fn pipeline_lowers_to_pipeline_call() {
        // `ls | grep foo` — every stage is a SYNC builtin (native ls, the
        // sync grep builtin), so the await-free pipeline dispatches to the
        // sync twin pipelineSync — identical fd0/fd1 stage swaps minus the
        // per-stage promise (the *Sync family, see src/shir.rs
        // SYNC_TWIN_CALLS).
        let json = to_json("ls | grep foo");
        assert!(json.contains("\"name\":\"pipelineSync\""));
        assert!(json.contains("\"type\":\"ArrowFunctionExpression\""));
        assert!(!json.contains("unsupported"));
        // an exec stage (a spawn) keeps the async pipeline
        let json2 = to_json("ls | awk '{print $1}'");
        assert!(json2.contains("\"name\":\"pipeline\""));
        assert!(json2.contains("\"type\":\"AwaitExpression\""));
    }

    #[test]
    fn command_substitution_uses_await_capture() {
        // Unquoted $(...) word-splits: captureWords returns an arg array.
        // `$(date)` — the captured command is a SYNC builtin, so the
        // await-free body dispatches to the sync twin captureWordsSync
        // (the inner spawn is already gone — the sync builtin twin runs
        // inside either way).
        let json = to_json("echo $(date)");
        assert!(json.contains("\"name\":\"captureWordsSync\""));
        assert!(json.contains("\"name\":\"builtin\""));
        assert!(json.contains("\"value\":\"date\""));
        assert!(!json.contains("unsupported"));
        // `$(ls)` is sync-builtin too (native ls) → captureSync, no await
        let json3 = to_json("echo $(ls)");
        assert!(!json3.contains("\"type\":\"AwaitExpression\""));
        assert!(json3.contains("\"name\":\"captureWordsSync\""));
        // a genuinely async captured command (a spawn) keeps the async
        // capture machinery: captureWords for unquoted, capture for quoted.
        let json4 = to_json("echo $(awk '{print $1}')");
        assert!(json4.contains("\"type\":\"AwaitExpression\""));
        assert!(json4.contains("\"name\":\"captureWords\""));
        let json2 = to_json("echo \"$(ls)\"");
        assert!(json2.contains("\"name\":\"captureSync\""));
        assert!(!json2.contains("captureWords"));
    }

    #[test]
    fn unsupported_constructs_are_marked() {
        // Regression guard: complex array/param/arith input must contain NO
        // sh2.unsupported (every construct the parser emits is lowered).
        let json = to_json(
            "numbers=(1 2 3)\necho ${#numbers[@]} ${numbers[1]} ${numbers[@]:1:2}\necho $((2+3)) ${x:-d}",
        );
        assert!(!json.contains("\"name\":\"unsupported\""));
    }

    #[test]
    fn bc_capture_lowers_to_native() {
        // Plan 8 (SH2_BC_NATIVE, default ON): `$(echo EXPR | bc)` capture
        // pipelines collapse to a native expression — a STATIC program
        // folds at compile time (src/bc.rs eval), the `sqrt($var)` form
        // (the primes is_prime pattern) becomes
        // `String(Math.floor(Math.sqrt(Number(...))))`. No spawn, no
        // pipeline/capture machinery, no async.
        let json = to_json("echo $(echo \"2+3\" | bc)");
        assert!(
            json.contains("\"value\":\"5\\n\""),
            "static bc program folds"
        );
        assert!(!json.contains("\"name\":\"capture\""));
        assert!(!json.contains("\"name\":\"pipeline\""));
        assert!(!json.contains("\"name\":\"exec\""));
        assert!(!json.contains("unsupported"));
        // the runtime-var sqrt form (store-bound $n → the native store
        // read — the read-builtin write is a plain setVar, exact as a
        // property read)
        let json2 = to_json("read n; echo \"$(echo \"sqrt($n)\" | bc)\"");
        assert!(json2.contains("\"name\":\"sqrt\""), "native sqrt expr");
        assert!(json2.contains("\"name\":\"floor\""));
        assert!(json2.contains("\"name\":\"vars\""));
        assert!(!json2.contains("\"name\":\"getVar\""));
        assert!(!json2.contains("\"name\":\"pipeline\""));
        assert!(!json2.contains("\"name\":\"capture\""));
        assert!(!json2.contains("\"name\":\"exec\""));
        assert!(!json2.contains("unsupported"));
        // the general var-operand form (`$sum + $i` — the in-loop bc
        // capture): native `String(Number(sum) + Number(i))`, no spawn
        let json3 =
            to_json("sum=0; for i in 1 2 3; do sum=$(echo \"$sum + $i\" | bc); done; echo $sum");
        assert!(json3.contains("\"name\":\"Number\""), "native var arith");
        assert!(json3.contains("\"operator\":\"+\""));
        assert!(!json3.contains("\"name\":\"pipeline\""));
        assert!(!json3.contains("\"name\":\"capture\""));
        assert!(!json3.contains("\"name\":\"exec\""));
        assert!(
            !json3.contains("\"name\":\"forLoop\""),
            "the loop goes native for-of"
        );
        assert!(!json3.contains("unsupported"));
        // `/` lowers to Math.trunc with a zero-divisor guard (bc aborts
        // with no stdout) — the guard is a `divisor === 0` comparison
        let json4 = to_json("a=7; b=2; echo $(echo \"$a / $b\" | bc)");
        assert!(json4.contains("\"name\":\"trunc\""), "scale-0 division");
        assert!(json4.contains("\"operator\":\"===\""), "zero-divisor guard");
        assert!(!json4.contains("\"name\":\"exec\""));
        assert!(!json4.contains("unsupported"));
        // `^` is bc POWER but bash-arith XOR — the var form must NOT
        // mis-parse it: the spawn stays
        let json5 = to_json("a=2; echo $(echo \"$a ^ 3\" | bc)");
        assert!(json5.contains("\"name\":\"exec\""), "^ keeps the spawn");
        assert!(!json5.contains("unsupported"));
    }

    #[test]
    fn case_lowers_to_switch_statement() {
        // `case` with simple patterns lifts to a native if/else-if chain
        // (no runtime dispatch) — see try_native_case in shir.rs.
        let json = to_json("case $x in a) echo a;; *) echo other;; esac");
        assert!(!json.contains("\"type\":\"SwitchStatement\""));
        assert!(!json.contains("\"name\":\"caseMatch\""));
        assert!(json.contains("\"type\":\"IfStatement\""));
        assert!(json.contains("\"name\":\"includes\"") || json.contains("\"operator\":\"===\""));
        assert!(!json.contains("unsupported"));
    }

    #[test]
    fn redirect_lowers_to_redirect_call() {
        // `echo hi > out.txt` lowers ALL the way to a native file write
        // (await sh2.fs.writeFile) — no redirect fd-swap, no dispatch.
        let json = to_json("echo hi > out.txt");
        assert!(json.contains("\"name\":\"writeFile\""));
        assert!(!json.contains("\"name\":\"redirect\""));
        assert!(!json.contains("\"name\":\"builtin\""));
        assert!(!json.contains("unsupported"));
        // non-echo bodies keep the runtime redirect — `ls` is a sync
        // builtin, so the await-free redirect dispatches to the sync twin
        // redirectSync (the *Sync family; the async `redirect` only when a
        // body/target awaits)
        let json2 = to_json("ls > out.txt");
        assert!(json2.contains("\"name\":\"redirectSync\""));
        // Property keys serialize as {key: Identifier{name}, value: Literal}.
        assert!(json2.contains("\"name\":\"mode\""));
        assert!(json2.contains("\"value\":\"w\""));
        assert!(json2.contains("\"name\":\"fd\""));
        assert!(json2.contains("\"value\":1"));
        assert!(json2.contains("\"type\":\"ObjectExpression\""));
        assert!(!json2.contains("unsupported"));
    }

    #[test]
    fn stderr_redirect_uses_fd_2() {
        let json = to_json("ls 2> err.txt");
        assert!(json.contains("\"value\":\"w\""));
        assert!(json.contains("\"value\":2"));
        assert!(!json.contains("unsupported"));
    }

    #[test]
    fn heredoc_lowers_to_redirect_with_body() {
        // `cat << 'EOF'` — the state-free heredoc cat fold (see
        // try_native_cat_heredoc): a literal quoted heredoc + builtin-cat
        // pair at the default stdout sink collapses to a native write of
        // the heredoc content — no redirect spec object, no dispatch.
        let json = to_json("cat << 'EOF'\nhi there\nEOF");
        assert!(json.contains("hi there"));
        assert!(!json.contains("\"value\":\"heredoc\""));
        assert!(json.contains("\"name\":\"stdout\""));
        assert!(!json.contains("unsupported"));
        // an interpolating heredoc (`$` in the UNQUOTED body) stays on
        // the runtime redirect + builtin pair (the quoted `<<'EOF'` form
        // is verbatim by construction and folds)
        let json2 = to_json("cat << EOF\nhi $name\nEOF");
        assert!(json2.contains("\"value\":\"heredoc\""));
        assert!(json2.contains("\"name\":\"builtin\""));
        assert!(json2.contains("\"value\":\"cat\""));
        assert!(!json2.contains("unsupported"));
    }

    #[test]
    fn function_lowers_to_define() {
        let json = to_json("greet() { echo hi; }");
        // the native define lowering: a direct `sh2.functions.set(name, fn)`
        // state write + `true`, no dispatch
        assert!(json.contains("\"name\":\"functions\""));
        assert!(json.contains("\"name\":\"set\""));
        assert!(json.contains("greet"));
        assert!(!json.contains("\"name\":\"define\""));
        assert!(!json.contains("unsupported"));
    }

    #[test]
    fn subshell_lowers_to_subshell_call() {
        // `(echo hi)` — the body is state-free (a native write + lastExit),
        // so the subshell collapses to a bare IIFE of the same body +
        // `sh2.lastExit === 0` (the runtime's exact return protocol) —
        // no state copy/restore, no dispatch.
        let json = to_json("(echo hi)");
        assert!(!json.contains("\"name\":\"subshellSync\""));
        assert!(json.contains("\"name\":\"lastExit\""));
        assert!(!json.contains("unsupported"));
        // a state-WRITING body (store write) keeps the sync twin
        let json1 = to_json("(x=1)");
        assert!(json1.contains("\"name\":\"subshellSync\""));
        assert!(!json1.contains("unsupported"));
        // a spawn inside keeps the async subshell
        let json2 = to_json("(awk '{print $1}')");
        assert!(json2.contains("\"name\":\"subshell\""));
        assert!(json2.contains("\"type\":\"AwaitExpression\""));
        // a state-free body CONTAINING self-contained machinery (a
        // pipeline of emits) folds too — the pipeline's fd swaps are
        // restored in its own finally, identical under the fold
        let json3 = to_json("(echo a | grep a)");
        assert!(!json3.contains("\"name\":\"subshellSync\""));
        assert!(!json3.contains("\"name\":\"pipelineSync\""));
        assert!(!json3.contains("unsupported"));
    }

    #[test]
    fn background_lowers_to_background_call() {
        let json = to_json("sleep 1 &");
        assert!(json.contains("\"name\":\"background\""));
        assert!(!json.contains("unsupported"));
    }

    #[test]
    fn env_var_prefix_is_exec_env_arg() {
        let json = to_json("FOO=bar echo hi");
        // `echo` is a sync builtin, so the env-carrying exec lowers to the
        // sync twin `sh2.builtin(..., {FOO: ...})` (the runtime applies the
        // command-scoped env exactly like the async exec path); a NON-builtin
        // name keeps the async exec call.
        assert!(json.contains("\"name\":\"builtin\""));
        assert!(json.contains("FOO"));
        assert!(!json.contains("unsupported"));
        // `grep` is a native sync builtin now (the file/stdin mini-grep),
        // so the env-carrying form also lowers to the sync twin; a name
        // OUTSIDE the sync-builtin set keeps the async exec call.
        let json2 = to_json("FOO=bar grep x");
        assert!(json2.contains("\"name\":\"builtin\""));
        assert!(json2.contains("FOO"));
        // `ls` is a native sync builtin too (the GNU-faithful native
        // listing — no spawn), so the env-carrying form lowers to the
        // sync twin as well; a genuinely external name (awk) keeps the
        // async exec call.
        let json3 = to_json("FOO=bar ls x");
        assert!(json3.contains("\"name\":\"builtin\""));
        assert!(json3.contains("FOO"));
        let json4 = to_json("FOO=bar awk x");
        assert!(json4.contains("\"name\":\"exec\""));
        assert!(json4.contains("FOO"));
    }

    #[test]
    fn compound_stages_lower_to_block_bodies() {
        // A while loop as a pipeline stage must not fall through to
        // `sh2.unsupported` — it becomes a block-bodied arrow.
        let json = to_json("yes | while read l; do echo $l; done | head -2");
        assert!(json.contains("\"name\":\"pipeline\""));
        assert!(json.contains("\"type\":\"BlockStatement\""));
        assert!(!json.contains("unsupported"));
    }

    #[test]
    fn control_flow_as_expression_operand() {
        let json = to_json("true && return");
        assert!(json.contains("\"name\":\"return\""));
        assert!(!json.contains("unsupported"));
    }

    #[test]
    fn intdecl_and_arith_stmt_lift_to_native() {
        // Plan 3: a bare `typeset -i i` is a numeric WITNESS (the
        // SH2_ASSUME_INTDECL lift) and a native `((i++))` / `let` statement
        // is an ARITH source — together they lift `i` to a native JS
        // binding: the per-iteration setVar/getVar chain disappears and the
        // statement lowers to a bare `++i` (no `sh2.*` calls at all).
        let json = to_json("typeset -i i\n((i++))");
        // the declare itself stays a runtime call (the attribute write);
        // the ARITH STATEMENT is fully native — no setVar/getVar/arith
        assert!(!json.contains("\"name\":\"setVar\""));
        assert!(!json.contains("\"name\":\"getVar\""));
        assert!(!json.contains("\"name\":\"arith\""));
        assert!(!json.contains("\"name\":\"exec\""));
        // `((i++))` emits a native postfix update — check for the operator
        assert!(json.contains("\"operator\":\"++\""));
        // a non-numeric source blocks the lift too (the runtime coerces
        // `i=foo` to 0 via the typeset attribute — a native binding would
        // desync from the store). The WRITE stays sh2.setVar (the int
        // coercion); the READ is the native store read (getVar's plain
        // path for an intVars name is the vars store — the attribute only
        // alters the write side).
        let json3 = to_json("typeset -i i\ni=foo\n((i++))");
        assert!(json3.contains("\"name\":\"setVar\""));
        assert!(!json3.contains("\"name\":\"getVar\""));
        // the same lift works through `let` statements without any declare
        let json4 = to_json("((i++))");
        assert!(!json4.contains("\"name\":\"setVar\""));
        assert!(!json4.contains("\"name\":\"getVar\""));
    }

    #[test]
    fn join_of_array_slice_chain_lowers_native() {
        // `${arr[@]:0:2}` in a template: the IR is `join(param("slice",
        // "arr", "0", "2"))` — for a provably-array-or-unset name
        // (array_only_written — the same proof the param slice emission
        // uses to pick its array path) the value is an array (or "" for
        // unset, and `[].slice().join(" ")` is "" — identical), so the
        // runtime join dispatch collapses to the native `.join(" ")`
        // method on the slice chain.
        let json = to_json("arr=(a b c d)\necho \"${arr[@]:0:2}\"");
        // no sh2.join dispatch
        assert!(!json.contains("\"name\":\"sh2\"},\"property\":{\"type\":\"Identifier\",\"name\":\"join\""));
        // the native chain: arr.slice(...).join(" ") — the array
        // itself (lowerNativeArrays replaced arrayItems with the bare
        // binding: the array is provably initialized at top level)
        assert!(!json.contains("\"name\":\"arrayItems\""));
        assert!(json.contains("\"name\":\"slice\""));
        assert!(json.contains("\"name\":\"join\""));
        assert!(json.contains("\"value\":\" \""));
        // a DYNAMIC plain-name slice (never-written name — the runtime
        // may see a scalar) keeps the runtime join
        let json2 = to_json("echo \"${s:0:2}\"");
        assert!(json2.contains("\"name\":\"sh2\"},\"property\":{\"type\":\"Identifier\",\"name\":\"join\""));
    }

    #[test]
    fn baked_subscript_read_uses_native_key() {
        // `${map[$k]}` with a lifted `k` (the loop var): the baked-text
        // store read (`sh2.getVar("map[$k]")` — the runtime resolves
        // `$k` from the STORE) rewrites to `sh2.arrayIndex("map", k)`
        // with the native binding — no store sync, no store round-trip.
        // SH2_ASSUME_SUBSCRIPT_KEYS-gated (see baked_subscript_read).
        let json = to_json("declare -A map\nmap[foo]=bar\nfor k in \"${!map[@]}\"; do echo \"${map[$k]}\"; done");
        assert!(json.contains("\"name\":\"arrayIndex\""));
        assert!(json.contains("\"value\":\"map\""));
        // no per-iteration store sync of the lifted loop var
        assert!(!json.contains("\"value\":\"k\""));
        assert!(!json.contains("\"name\":\"getVar\""));
        // `[@]`/`[*]` whole-array forms and PIPESTATUS never rewrite
        // (the join / pipeStatuses arms are getVar-special)
        let json2 = to_json("declare -A map\nmap[a]=1\necho \"${map[*]}\"\necho \"${PIPESTATUS[0]}\"");
        assert!(json2.contains("\"name\":\"getVar\""));
    }

    #[test]
    fn arith_len_refs_lower_let_and_while_native() {
        // The `${#name[@]}` / `${#name}` arith-length refs: the runtime's
        // evalArith arms are `Number(sh.arrayLen(name)) || 0` and
        // `sh.getVar(name).length` — EXACT native leafs (the length
        // substitution always yields a digit string, so no unset-var
        // deletion gate applies; no SH2_ASSUME option needed). A
        // `(( ${#arr[@]} > 5 ))` condition lowers to the native
        // comparison: no `let` builtin dispatch, no per-iteration text
        // parse.
        let json = to_json("(( ${#arr[@]} > 5 ))");
        assert!(!json.contains("\"name\":\"builtin\""));
        assert!(json.contains("\"name\":\"arrayLen\""));
        assert!(json.contains("\"operator\":\">\""));
        // `${#s}` — the scalar length is the value's `.length` (the
        // store read, not a getVar call, for a plain store name)
        let json2 = to_json("s=abc; (( ${#s} > 2 ))");
        assert!(!json2.contains("\"name\":\"builtin\""));
        assert!(!json2.contains("\"name\":\"getVar\""));
        assert!(json2.contains("\"name\":\"length\""));
        // a statement-level `while (( i < ${#args[@]} ))` lowers to the
        // NATIVE while machinery (__sh2_loop_ran protocol) with the
        // arrayLen cond — no runtime loop, no let dispatch
        let json3 = to_json("while (( i < ${#args[@]} )); do (( i++ )); done");
        assert!(json3.contains("__sh2_loop_ran"));
        assert!(!json3.contains("\"name\":\"builtin\""));
        assert!(json3.contains("\"name\":\"arrayLen\""));
        // `${#arr[i]}` — the ELEMENT length (runtime-only shape): keeps
        // the runtime let dispatch
        let json4 = to_json("(( ${#arr[i]} > 5 ))");
        assert!(json4.contains("\"name\":\"builtin\""));
    }

    #[test]
    fn shopt_and_cstyle_for_lower() {
        let json = to_json("shopt -s extglob");
        // the native shopt lowering: a direct `sh2.shoptState.set(opt, en)`
        // state write + `true`, no dispatch
        assert!(json.contains("\"name\":\"shoptState\""));
        assert!(!json.contains("\"name\":\"shopt\""));
        assert!(!json.contains("unsupported"));
        // the body is `echo $i` — a sync builtin call (no await) → the
        // c-style loop lowers to the SYNC runtime twin.
        let json2 = to_json("for ((i=0; i<3; i++)); do echo $i; done");
        assert!(json2.contains("\"name\":\"cstyleForSync\""));
        assert!(!json2.contains("\"name\":\"cstyleFor\""));
        assert!(!json2.contains("unsupported"));
    }

    #[test]
    fn param_default_cmdsub_defaults_lower_native() {
        // The ${VAR:-$(cmd)} family: the baked-text default the runtime
        // would run through expandWord (spawning `bash -c` for the
        // cmdsub) lowers to native reads — `$(pwd)` → `sh2.cwd` (a
        // property read, no call), `$(whoami)` → the value twin. The
        // primary read is one `sh2.getVar` (the `_g` single-eval wrap).
        let json = to_json("echo \"${PWD:-$(pwd)}\"");
        // the primary read is the runtime special twin `sh2.cwd` (the
        // `_g` single-eval wrap — the runtime's getVar("PWD") answers
        // `this.cwd`, never the vars/env store)
        assert!(json.contains("\"name\":\"cwd\""));
        assert!(json.contains("\"name\":\"_g\""));
        assert!(!json.contains("\"name\":\"param\""));
        assert!(!json.contains("unsupported"));
        let json2 = to_json("echo \"${USER:-$(whoami)}\"");
        // USER is env-resident (never written): the primary read is the
        // native store read `sh2.vars.USER ?? env.USER ?? ''` (the
        // runtime's exact plain path)
        assert!(json2.contains("\"name\":\"USER\""));
        assert!(json2.contains("\"name\":\"whoami\""));
        assert!(!json2.contains("\"name\":\"param\""));
        // the tilde default `${HOME:-$(echo ~)}` is the native store
        // read (the runtime's tilde rule is getVar("HOME") — vars then
        // env fallback — the property read is the same value without
        // the dispatch)
        let json3 = to_json("echo \"${HOME:-$(echo ~)}\"");
        assert!(json3.contains("\"name\":\"HOME\""));
        assert!(!json3.contains("\"name\":\"param\""));
        assert!(!json3.contains("unsupported"));
    }

    #[test]
    fn nested_param_default_chain_lowers_to_getvar_ternaries() {
        // ${var:-${default:-${fallback:-$(echo "computed")}}} — the
        // nested chain: the runtime's expandWord would spawn bash for
        // the $(echo) cmdsub; the native is a getVar ternary chain (one
        // per level, `_g`-scratched) with the literal echo default. No
        // param call, no spawn.
        let json = to_json("echo \"${var:-${default:-${fallback:-$(echo \"computed\")}}}\"");
        // the never-written `var` level folds to the lift-known constant
        // "" (its store read); the live levels read via the native
        // store read (env-fallback property reads — no getVar dispatch)
        assert!(!json.contains("\"name\":\"getVar\""));
        assert!(json.contains("computed"));
        assert!(!json.contains("\"name\":\"param\""));
        assert!(!json.contains("unsupported"));
        // the array-slice default ${default[@]:0:2} → the native store
        // read (exact for unset/scalar operands — the documented
        // assumption); the ${array[${index}]} PRIMARY stays a runtime
        // getVar (a subscript name — not a plain ident)
        let json2 = to_json("echo \"${array[${index}]:-${default[@]:0:2}}\"");
        assert_eq!(json2.matches("\"name\":\"getVar\"").count(), 1);
        assert!(!json2.contains("\"name\":\"param\""));
        // a ${NAME} plain-ref default lowers to the native store read too
        let json3 = to_json("echo ${MOUNTPOINT:-${NAME}}");
        // the never-written MOUNTPOINT level folds to the constant ""
        // (its store read); the live NAME level reads natively
        assert!(!json3.contains("\"name\":\"getVar\""));
        assert!(!json3.contains("\"name\":\"param\""));
    }

    #[test]
    fn param_error_question_lowers_to_stderr_write_and_exit() {
        // ${x:?msg} — the unset/empty error: the native is the runtime's
        // exact `process.stderr.write("bash: x: msg\n"); process.exit(1)`
        // sequence (the corpus gate ignores stderr; the exit code is the
        // verdict). A never-written var folds to its lift-known constant
        // "" (the error path fires); a WRITTEN var reads via getVar (the
        // `_g` single-eval wrap).
        let json = to_json("echo \"${var:?error message}\"");
        assert!(json.contains("process"));
        assert!(json.contains("stderr"));
        assert!(json.contains("\"name\":\"exit\""));
        assert!(!json.contains("\"name\":\"param\""));
        assert!(!json.contains("unsupported"));
        // empty message → the `name: parameter null or not set` default
        let json2 = to_json("echo \"${var:?}\"");
        assert!(json2.contains("parameter null or not set"));
        assert!(!json2.contains("\"name\":\"param\""));
        // a STORE-BOUND var (read-builtin — never lifted) keeps the
        // LIVE read (the `_g` single-eval wrap): the native store read
        // `sh2.vars.v ?? env ?? ''` — the runtime's exact plain path
        // (a getVar CALL would be a dispatch)
        let json4 = to_json("read v <<< \"x\"\necho \"${v:?err}\"");
        assert!(json4.contains("\"name\":\"vars\""));
        assert!(!json4.contains("\"name\":\"param\""));
        let json5 = to_json("v=1\necho \"${v:?err}\"");
        assert!(!json5.contains("\"name\":\"getVar\""));
        assert!(!json5.contains("\"name\":\"param\""));
        // a DYNAMIC message (expandWord would expand the ref) keeps the
        // runtime param call
        let json3 = to_json("echo \"${var:?$other}\"");
        assert!(json3.contains("\"name\":\"param\""));
    }

    #[test]
    fn param_assign_default_writes_store_natively() {
        // ${maybe:=default} — the store `:=` write: the runtime's
        // getVar + expandWord + setVar lowers to the same getVar (the
        // `_g` wrap) + a REAL sh2.setVar call (the store authority) +
        // the value — no dispatch, no text parse. The unset-clean name
        // takes the native plain-object store paths everywhere: the
        // primary read is `sh2.vars.maybe ?? env ?? ''` and the write
        // is `sh2.vars.maybe = "default"` (the runtime setVar's plain
        // path — no attributes, no env sync).
        let json = to_json("unset maybe\necho \"${maybe:=default}\"");
        assert!(json.contains("\"name\":\"vars\""));
        assert!(!json.contains("\"name\":\"getVar\""));
        assert!(!json.contains("\"name\":\"setVar\""));
        assert!(!json.contains("\"name\":\"param\""));
        assert!(!json.contains("unsupported"));
    }

    #[test]
    fn export_unset_and_store_declare_lower_to_native_store_paths() {
        // `export NAME=VALUE` — the runtime builtin's exact writes
        // (vars + process.env + the exported set) minus the dispatch:
        // `(sh2.vars.DEBUG = "1", process.env.DEBUG = "1",
        // sh2.exported.add("DEBUG"), sh2.lastExit = 0)` — no builtin
        // call. The bare `export NAME` form gets the conditional env
        // sync (the runtime's `a in vars` check).
        let json = to_json("export DEBUG=1");
        assert!(json.contains("\"name\":\"exported\""));
        assert!(json.contains("\"name\":\"DEBUG\""));
        assert!(!json.contains("\"name\":\"builtin\""));
        let json2 = to_json("SHELL_VAR=hello\nexport SHELL_VAR");
        assert!(json2.contains("\"name\":\"exported\""));
        assert!(!json2.contains("\"name\":\"builtin\""));
        // `unset NAME` — the two native deletes (vars + env); a
        // later read is the native plain-object store read (the unset
        // name carries no attributes — the store access is exact).
        let json3 = to_json("unset x\necho \"$x\"");
        assert!(json3.contains("\"operator\":\"delete\""));
        assert!(json3.contains("\"name\":\"vars\""));
        assert!(!json3.contains("\"name\":\"builtin\""));
        assert!(!json3.contains("\"name\":\"getVar\""));
        // a STORE-BOUND `local i=0` (the var stays store-bound via the
        // baked-subscript mark) — the native `sh2.vars.i = "0"` store
        // write, no builtin dispatch (the lifted-name twin keeps the
        // identifier write).
        let json4 = to_json("f() { local i=0; echo ${arr[$i]}; }; f");
        assert!(json4.contains("\"name\":\"vars\""));
        assert!(!json4.contains("\"value\":\"local\""));
    }

    #[test]
    fn deterministic_output() {
        let input = "x=1\nif [ -f /tmp/x ]; then echo $x; fi\nls | wc -l";
        let commands = Parser::new(input).parse().unwrap();
        let a = serde_json::to_string(&ast_to_estree(&commands)).unwrap();
        let b = serde_json::to_string(&ast_to_estree(&commands)).unwrap();
        assert_eq!(a, b);
    }

    #[test]
    fn longoption_dollar_brace_expands() {
        // `--x="${X}"`: the LongOption lexer path merges the quoted string
        // as raw text, arriving as the bare literal `--x=${X}`. The transform
        // splits it so the parameter expansion is evaluated — natively now
        // (X is a lifted string binding: `${X}` → bare `X`).
        let json = to_json("X=test; echo --x=\"${X}\"");
        assert!(json.contains("--x="));
        assert!(json.contains("\"type\":\"Identifier\",\"name\":\"X\""));
        assert!(!json.contains("\"name\":\"param\""));
        assert!(!json.contains("\"name\":\"unsupported\""));
        // Single-quoted `${x}` (no `=`) stays literal; `\\${x}` (escaped
        // dollar) keeps its backslash — both must NOT be expanded.
        let json2 = to_json("echo '${x}'");
        assert!(json2.contains("${x}"));
        assert!(!json2.contains("\"name\":\"param\""));
        let json3 = to_json("echo \\${x}");
        assert!(json3.contains("${x}"));
        assert!(!json3.contains("\"name\":\"param\""));
        assert!(!json3.contains("\"name\":\"unsupported\""));
    }

    #[test]
    fn escaped_quote_artifact_merges_words() {
        // `echo 'test'\''test'` is ONE bash word (`test'test`); the lexer
        // splits it into `'test'` + `\''test'`. The transform merges the
        // `\'`-prefixed arg into its predecessor (dropping the backslash).
        let json = to_json("echo 'test'\\''test'");
        assert!(json.contains("test'test"));
        assert!(!json.contains("\\'test"));
        assert!(!json.contains("\"name\":\"unsupported\""));
    }

    #[test]
    fn dollardollar_stays_inside_redirect_target() {
        // `cmd 2>/tmp/x.$$`: the parser cuts the `$$` off the redirect target
        // and pushes it onto the args. The transform re-attaches it to the
        // target (interpolation with the pid) and drops the bogus arg.
        let json = to_json("realpath /bin 2>/tmp/realpath_stderr.$$").replace('\\', "");
        assert!(json.contains("/tmp/realpath_stderr."));
        // `$$` lowers to a direct `String(process.pid)` read, no dispatch
        assert!(json.contains("process"));
        assert!(json.contains("pid"));
        assert!(!json.contains("\"name\":\"getVar\""));
        // the exec args must NOT contain the bare `$` expansion anymore
        let args = json
            .split("\"name\":\"exec\"")
            .nth(1)
            .map(|s| &s[..s.find("\"optional\":false}").unwrap_or(0)])
            .unwrap_or("");
        assert!(!args.contains("getVar") || args.contains("/tmp/realpath_stderr."));
        assert!(!json.contains("\"name\":\"unsupported\""));
    }

    #[test]
    fn dollardollar_arg_joins_into_one_word() {
        // `cat /tmp/x.$$` — the `$$` arg is re-joined with the literal prefix
        // into a single interpolated word (one exec arg, not two).
        let json = to_json("cat /tmp/realpath_stderr.$$").replace('\\', "");
        assert!(json.contains("/tmp/realpath_stderr."));
        // `$$` lowers to a direct `String(process.pid)` read, no dispatch
        assert!(json.contains("process"));
        assert!(json.contains("pid"));
        assert!(!json.contains("\"name\":\"getVar\""));
        assert!(!json.contains("\"name\":\"unsupported\""));
    }

    #[test]
    fn break_inside_loop_lowers_to_sh2_call() {
        // `break`/`continue` inside a loop body is emitted INSIDE an arrow
        // function, so it must become a sh2.break() call, never a native
        // `break;` statement (illegal JS in an arrow body).
        let json = to_json("while true; do echo x; break; done");
        assert!(json.contains("\"name\":\"break\""));
        assert!(!json.contains("\"type\":\"BreakStatement\""));
        assert!(!json.contains("unsupported"));
        let json2 = to_json("for i in a b; do continue; done");
        assert!(json2.contains("\"name\":\"continue\""));
        assert!(!json2.contains("\"type\":\"ContinueStatement\""));
    }

    #[test]
    fn case_breaks_stay_native() {
        // Source break/continue inside a case must exit the ENCLOSING loop
        // (bash semantics) — the lifted if-chain turns them into runtime
        // signals, so no native BreakStatement appears in a case body.
        let json = to_json("case $x in a) echo a;; *) echo b;; esac");
        assert!(!json.contains("\"type\":\"BreakStatement\""));
    }

    #[test]
    fn top_level_return_lowers_to_sh2_call() {
        // A top-level `return` is illegal in ESM; it becomes sh2.return().
        let json = to_json("case $1 in foo) return 0;; esac");
        assert!(json.contains("\"name\":\"return\""));
        assert!(!json.contains("\"type\":\"ReturnStatement\""));
        assert!(!json.contains("unsupported"));
    }

    #[test]
    fn unterminated_param_expansion_drops_command() {
        // `echo "${var:?unset"` (missing closing `}`) is a bash parse error
        // (exit 2, nothing runs) — the parser now REJECTS the unterminated
        // expansion outright (the old literal-`${` artifact + drop-the-
        // command transform silently exited 0; the CLI's parse-error
        // fallback reproduces bash's verdict). The artifact detection stays
        // for other producers (heredoc re-parses), but the canonical parse
        // must fail.
        let err = crate::Parser::new("echo \"${var:?unset\"")
            .parse()
            .expect_err("unterminated `${` must be a parse error");
        assert!(
            format!("{}", err).contains("unterminated"),
            "unexpected error: {}",
            err
        );
        // Legit single-quoted `${x}` text (closing brace present) survives.
        let json2 = to_json("echo '${x}'");
        assert!(json2.contains("${x}"));
        assert!(!json2.contains("\"name\":\"unsupported\""));
        // Bare `${` (escaped-dollar artifact, `\\${`) is only the opener:
        // not an unterminated expansion, keep the command.
        let json3 = to_json("echo '${'");
        assert!(json3.contains("${"));
        assert!(!json3.contains("\"name\":\"unsupported\""));
    }

    #[test]
    fn env_assignment_with_redirect_keeps_env_on_command() {
        // frontends-ifs: `IFS=, read a b c <<< "1,2,3"` — the env vars
        // must scope the actual command (a redirect-wrapped simple
        // command), NOT split into a sibling `true` no-op (the old
        // behavior left read without the env — the read fell back to
        // whitespace IFS).
        let json = to_json("IFS=, read a b c <<< \"1,2,3\"");
        // the redirect wraps the env-carrying read (the env stays on the
        // command; the no-op `true` split is gone) — the await-free body
        // + literal herestring target dispatch to the sync twin
        // redirectSync
        assert!(json.contains("\"name\":\"redirectSync\""));
        assert!(json.contains("\"name\":\"builtin\""));
        assert!(json.contains("\"value\":\"read\""));
        // the env object's property key is an Identifier (prop() renders
        // `key: {type: Identifier, name: IFS}`)
        assert!(json.contains("\"name\":\"IFS\""));
        assert!(json.contains("\"value\":\",\""));
        // the env must NOT land on a separate `true` command
        assert!(!json.contains("\"value\":\"true\""));
        assert!(!json.contains("\"name\":\"unsupported\""));
    }

    #[test]
    fn local_scope_shadows_outer_binding() {
        // fish-sh-go local-scope request: `local v=1` inside a function
        // must NOT leak into the outer v (the runtime's flat store model
        // leaks; the per-function local lift emits a native `let` inside
        // the define arrow). The emitted function body must contain a
        // `let v` (block-scope shadow), and the module binding must NOT
        // be the write target of the decl.
        let json = to_json("f() { local v=1; echo \"inner=$v\"; }; v=2; f; echo \"outer=$v\"");
        assert!(json.contains("\"kind\":\"let\""));
        assert!(json.contains("\"name\":\"v\""));
        // the first decl inside the arrow is a `let v = 1` VariableDeclaration
        assert!(json.contains("\"type\":\"VariableDeclaration\""));
        // local-lifted arith: `local i=3; ((i++))` writes the native
        // binding, not the store (no setVar for the incdec)
        let json2 = to_json("g() { local i=3; ((i++)); echo \"i=$i\"; }; g");
        assert!(!json2.contains("\"name\":\"setVar\""));
        assert!(json2.contains("\"name\":\"i\""));
    }

    #[test]
    fn process_substitution_lowers_without_unsupported() {
        // <(cmd) as an argument position: the redirect becomes a here-string
        // (captured producer stdout) and a materialized-path argument is
        // appended for the runtime to turn into a temp file. `<(echo a)`
        // is a pure echo capture — lowered natively (the joined literal
        // provably cannot end with a newline, so even the trimCapture
        // wrapper drops), no async capture machinery.
        let json = to_json("diff <(echo a) <(echo b)");
        assert!(!json.contains("\"name\":\"unsupported\""));
        assert!(!json.contains("\"value\":\"unsupported\""));
        assert!(!json.contains("\"name\":\"trimCapture\""));
        // diff is a native sync builtin now (the GNU-faithful gnuDiff —
        // no spawn), so the producer-capture form lowers to the sync
        // builtin dispatch; an external name (awk) keeps the async exec.
        let jsona = to_json("awk <(echo a) <(echo b)");
        assert!(jsona.contains("\"name\":\"exec\""));
        // mapfile is stdin-only: no appended path argument, still no gate leak
        // (the producer's capture is lowered as a here-string fd-0 redirect
        // feeding the sync mapfile builtin — no async capture machinery).
        let json2 = to_json("mapfile -t lines < <(printf 'x\\ny\\n')");
        assert!(!json2.contains("\"name\":\"unsupported\""));
        assert!(!json2.contains("\"value\":\"unsupported\""));
        // the producer + mapfile body are both sync builtins → the
        // await-free redirect dispatches to the sync twin redirectSync
        assert!(json2.contains("\"name\":\"redirectSync\""));
        assert!(json2.contains("\"name\":\"builtin\""));
    }
}

#[cfg(test)]
mod last_exit_hoist_tests {
    use super::*;
    use crate::Parser;

    fn to_json(input: &str) -> String {
        let commands = Parser::new(input).parse().unwrap();
        serde_json::to_string(&ast_to_estree(&commands)).unwrap()
    }

    fn body(json: &str) -> serde_json::Value {
        let v: serde_json::Value = serde_json::from_str(json).unwrap();
        v["body"].clone()
    }

    fn lastexit_count(v: &serde_json::Value) -> usize {
        serde_json::to_string(v)
            .unwrap()
            .matches("\"name\":\"lastExit\"")
            .count()
    }

    /// `sh2.lastExit = <number>` at the END of the program body?
    fn last_stmt_is_lastexit_write(json: &str) -> bool {
        let b = body(json);
        let stmts = b.as_array().unwrap();
        let Some(last) = stmts.last() else { return false };
        last["expression"]["type"] == "AssignmentExpression"
            && last["expression"]["left"]["object"]["name"] == "sh2"
            && last["expression"]["left"]["property"]["name"] == "lastExit"
    }

    /// The exemplar — `for i in $(seq 1 10000)` with an if/else whose both
    /// branches end in `sh2.lastExit = 0`:
    ///
    ///   for i in `seq 1 10000`; do
    ///     if echo $((i*i)) | grep 1337 >/dev/null 2>/dev/null; then echo $i; fi
    ///   done
    ///
    /// Phase 1 lifts the write out of both branches of the if; phase 2 then
    /// lifts it out of the native range loop. Result: the if has no
    /// alternate, the loop body has NO lastExit mention, and the program
    /// ends with a single `sh2.lastExit = 0`.
    #[test]
    fn sqrt1337_lifts_from_if_branches_then_loop() {
        let json = to_json(
            "for i in `seq 1 10000`\n\
             do\n\
             \tif echo $((i*i)) | grep 1337 > /dev/null 2> /dev/null\n\
             \tthen\n\
             \t\techo $i\n\
             \tfi\n\
             done",
        );
        let v: serde_json::Value = serde_json::from_str(&json).unwrap();
        // exactly ONE lastExit write left, and it is the program-final
        // statement (hoisted out of the loop entirely)
        assert_eq!(lastexit_count(&v), 1, "one write remains: {json}");
        assert!(
            last_stmt_is_lastexit_write(&json),
            "program-final write: {json}"
        );
        // the loop is a native range for whose body has no lastExit mention
        let stmts = v["body"].as_array().unwrap();
        let for_stmt = &stmts[stmts.len() - 2];
        assert_eq!(for_stmt["type"], "ForStatement");
        assert!(
            !serde_json::to_string(&for_stmt["body"])
                .unwrap()
                .contains("lastExit"),
            "loop body lastExit-free: {json}"
        );
        // the if lost its else (the false-path write was hoisted away)
        let if_stmt = &for_stmt["body"]["body"][0];
        assert_eq!(if_stmt["type"], "IfStatement");
        assert!(
            if_stmt["alternate"].is_null() || if_stmt["alternate"] == serde_json::Value::Null,
            "no else: {json}"
        );
        assert!(!json.contains("unsupported"));
    }

    /// The if-hoist is independent of the loop: a top-level
    /// `if c; then echo hi; fi` also collapses its synthesized false-path
    /// write into a single post-if write. (The `false` TEST expression
    /// keeps its own status recording — `(sh2.lastExit = 1, false)` —
    /// that write is the test command's status, not a branch tail.)
    #[test]
    fn top_level_if_common_tail_lifted() {
        let json = to_json("if false; then echo yes; fi");
        let v: serde_json::Value = serde_json::from_str(&json).unwrap();
        let stmts = v["body"].as_array().unwrap();
        assert_eq!(stmts.len(), 2, "if + hoisted write: {json}");
        assert_eq!(stmts[0]["type"], "IfStatement");
        assert!(
            stmts[0]["alternate"].is_null(),
            "false-path else collapsed: {json}"
        );
        // the then-branch's tail sequence lost its status write
        let cons = serde_json::to_string(&stmts[0]["consequent"]).unwrap();
        assert!(
            !cons.contains("lastExit"),
            "then-branch status write lifted: {json}"
        );
        assert!(
            last_stmt_is_lastexit_write(&json),
            "post-if write: {json}"
        );
        assert!(!json.contains("unsupported"));
    }

    /// A `$?` read anywhere in the loop body vetoes the LOOP hoist (the
    /// pre-loop value would be observed mid-loop) but NOT the if hoist —
    /// the write stays as the body's tail.
    #[test]
    fn loop_hoist_vetoed_by_body_read() {
        // the if's both-branches write still collapses; the loop keeps it
        // because `x=$?` reads lastExit in the body
        let json = to_json(
            "for i in `seq 1 3`\n\
             do\n\
             \tx=$?\n\
             \tif echo $((i*i)) | grep 1337 > /dev/null 2> /dev/null\n\
             \tthen\n\
             \t\techo $i\n\
             \tfi\n\
             done",
        );
        let v: serde_json::Value = serde_json::from_str(&json).unwrap();
        let stmts = v["body"].as_array().unwrap();
        let for_stmt = &stmts[stmts.len() - 1];
        assert_eq!(for_stmt["type"], "ForStatement");
        // the loop body still carries the tail write (no post-loop write)
        assert!(
            serde_json::to_string(&for_stmt["body"])
                .unwrap()
                .contains("lastExit"),
            "write stays in the body: {json}"
        );
        assert!(
            !last_stmt_is_lastexit_write(&json),
            "no post-loop hoist: {json}"
        );
        assert!(!json.contains("unsupported"));
    }

    /// The loop-hoist on a native range for requires the loop to provably
    /// run ≥ 1 time — an empty range (`seq 5 1`) must NOT hoist the write
    /// out (0 iterations would leave `$?` = the pre-loop value in bash).
    #[test]
    fn loop_hoist_vetoed_for_empty_range() {
        let json = to_json(
            "for i in `seq 5 1`\n\
             do\n\
             \tif echo $((i*i)) | grep 1337 > /dev/null 2> /dev/null\n\
             \tthen\n\
             \t\techo $i\n\
             \tfi\n\
             done",
        );
        let v: serde_json::Value = serde_json::from_str(&json).unwrap();
        assert!(
            !last_stmt_is_lastexit_write(&json),
            "no post-loop hoist for an empty range: {json}"
        );
        // the if-hoist still fired inside the loop body
        let stmts = v["body"].as_array().unwrap();
        let for_stmt = &stmts[stmts.len() - 1];
        let body = serde_json::to_string(&for_stmt["body"]).unwrap();
        assert!(body.contains("lastExit"), "if-tail stays in the body: {json}");
        assert!(!json.contains("unsupported"));
    }

    /// Different exit values in the two branches (a failing last command on
    /// one path) veto the if-hoist — the status after the if differs by
    /// path, so the write cannot move.
    #[test]
    fn differing_branch_tails_not_lifted() {
        let json = to_json("if false; then false; else echo hi; fi");
        let v: serde_json::Value = serde_json::from_str(&json).unwrap();
        // the if keeps a lastExit write in at least one branch
        assert!(
            lastexit_count(&v) >= 1,
            "branch status writes kept: {json}"
        );
        assert!(
            !last_stmt_is_lastexit_write(&json),
            "no unconditional post-if write when statuses differ: {json}"
        );
        assert!(!json.contains("unsupported"));
    }
}

#[cfg(test)]
mod migrated_passes_tests {
    use super::*;
    use crate::Parser;

    fn to_json(input: &str) -> String {
        let commands = Parser::new(input).parse().unwrap();
        serde_json::to_string(&ast_to_estree(&commands)).unwrap()
    }

    fn count(json: &str, needle: &str) -> usize {
        json.matches(needle).count()
    }

    /// lowerNativeArrays — a provably-static array drops its runtime
    /// store calls: setArray → `let arr = [..]`, element reads →
    /// `(arr[1] !== undefined ? arr[1] : "")`, `${#arr[@]}` → `arr.length`,
    /// `${arr[@]}` → the bare array (the emitter's `[..].flat().join`
    /// path joins it with spaces).
    #[test]
    fn static_array_lowers_to_native() {
        let json = to_json("arr=(alpha beta gamma); echo ${arr[1]}");
        assert_eq!(count(&json, "\"name\":\"setArray\""), 0, "{json}");
        assert!(json.contains("\"type\":\"VariableDeclaration\""), "{json}");
        assert!(json.contains("\"name\":\"arr\""), "{json}");
        assert!(
            json.contains("\"operator\":\"!==\"") && json.contains("\"name\":\"undefined\""),
            "element read → (arr[i] !== undefined ? arr[i] : \"\"): {json}"
        );
        assert!(!json.contains("unsupported"));
    }

    #[test]
    fn array_length_and_items_lower_to_native() {
        let json = to_json("arr=(a b c); echo ${#arr[@]}; echo \"${arr[@]}\"");
        assert_eq!(count(&json, "\"name\":\"arrayLen\""), 0, "{json}");
        assert_eq!(count(&json, "\"name\":\"arrayItems\""), 0, "{json}");
        assert!(json.contains("\"name\":\"length\""), "len → arr.length: {json}");
        assert!(json.contains("\"name\":\"arr\""), "items → bare arr: {json}");
        assert!(!json.contains("unsupported"));
    }

    /// Whole-var reads (`$arr`), writes (`arr[1]=x`), refs inside a
    /// script function, and computed subscripts all veto the lowering —
    /// the array stays on the runtime store.
    #[test]
    fn array_lowering_is_conservative() {
        // whole-var read
        let j1 = to_json("arr=(a b c); echo $arr");
        assert_eq!(count(&j1, "\"name\":\"setArray\""), 1, "whole read keeps runtime: {j1}");
        // element write
        let j2 = to_json("arr=(a b c); arr[1]=x; echo ${arr[1]}");
        assert_eq!(count(&j2, "\"name\":\"setArray\""), 1, "write keeps runtime: {j2}");
        // ref inside a script function (deferred invocation / shadowing)
        let j3 = to_json("f() { echo ${arr[1]}; }; arr=(a b c); f");
        assert_eq!(count(&j3, "\"name\":\"setArray\""), 1, "in-function keeps runtime: {j3}");
        // computed subscript (runtime evalArith + negative wrap)
        let j4 = to_json("arr=(a b c); i=1; echo ${arr[$i]}");
        assert_eq!(count(&j4, "\"name\":\"setArray\""), 1, "computed index keeps runtime: {j4}");
        // a nested (conditional) setArray can't become a top-level `let`
        let j5 = to_json("if true; then arr=(a b); fi; echo ${arr[1]}");
        assert_eq!(count(&j5, "\"name\":\"setArray\""), 1, "nested setArray keeps runtime: {j5}");
        for j in [&j1, &j2, &j3, &j4, &j5] {
            assert!(!j.contains("unsupported"));
        }
    }

    /// A single provably-scalar echo arg drops the array/join machinery:
    /// `[i].flat().join(" ")` is exactly `i`, so `echo $i` writes
    /// `i + "\n"` (the No-Space/numeric skip already scalarized the
    /// field-split, leaving a stale flat flag). An ARRAY-VALUED single
    /// arg (${arr[@]}) keeps the flat/join splice.
    #[test]
    fn single_scalar_echo_arg_drops_join_machinery() {
        let json = to_json("i=5; echo $i");
        let v: serde_json::Value = serde_json::from_str(&json).unwrap();
        let s = serde_json::to_string(&v).unwrap();
        assert!(!s.contains("\"name\":\"flat\""), "no flat: {json}");
        assert!(!s.contains("\"name\":\"join\""), "no join: {json}");
        assert!(!s.contains("\"type\":\"ArrayExpression\""), "no array: {json}");
        assert!(s.contains("\"name\":\"i\""), "bare numeric arg: {json}");
        assert!(!json.contains("unsupported"));
        // the loop-counter case from sqrt1337.sh
        let json2 = to_json("for i in `seq 1 3`; do echo $i; done");
        assert!(!json2.contains("\"name\":\"flat\""), "loop counter: no flat: {json2}");
        assert!(!json2.contains("\"name\":\"join\""), "loop counter: no join: {json2}");
        // an ARRAY-VALUED single arg keeps the flat/join splice
        let json3 = to_json("arr=(a b c); echo \"${arr[@]}\"");
        assert!(json3.contains("\"name\":\"flat\""), "array arg keeps flat: {json3}");
        assert!(json3.contains("\"name\":\"join\""), "array arg keeps join: {json3}");
        for j in [&json, &json2, &json3] {
            assert!(!j.contains("unsupported"));
        }
    }

    /// dropDeadFlags — every statement except the program's last has a
    /// dead success flag: `(cmd, true)` unwraps to `cmd` (the flag was
    /// already stripped from branch tails by the lastExit hoist).
    #[test]
    fn dead_flags_dropped_except_program_last() {
        let json = to_json("if false; then echo yes; fi; echo after");
        let v: serde_json::Value = serde_json::from_str(&json).unwrap();
        let stmts = v["body"].as_array().unwrap();
        // the if's consequent is a bare write — no sequence, no flag
        let cons = &stmts[0]["consequent"]["body"][0];
        assert_eq!(cons["expression"]["type"], "CallExpression", "{json}");
        // the program's LAST statement keeps its sequence (its value is
        // the exit flag for jtsh's runViaTranspiler)
        let last = stmts.last().unwrap();
        assert_eq!(last["expression"]["type"], "SequenceExpression", "{json}");
        assert!(!json.contains("unsupported"));
    }

    #[test]
    fn dead_flags_unwrap_one_element_sequences() {
        // a non-last statement that is a bare `(flag)` (1-element seq)
        // after the lastExit hoist unwraps/drops; the last keeps it
        let json = to_json("echo one; echo two");
        let v: serde_json::Value = serde_json::from_str(&json).unwrap();
        let stmts = v["body"].as_array().unwrap();
        assert_eq!(
            stmts[0]["expression"]["type"], "CallExpression",
            "non-last statement unwrapped: {json}"
        );
        assert_eq!(
            stmts.last().unwrap()["expression"]["type"], "SequenceExpression",
            "last statement keeps its flag: {json}"
        );
        assert!(!json.contains("unsupported"));
    }
}

// ── dead top-level declaration elimination ────────────────────────────
//
// The lifted-numeric/string declarations (`let x = 0` / `let x = ""` at
// program top) exist so bash's unset-var semantics hold at the top
// level. A seq-range for (`for (let i = lo; i <= hi; i++)`) declares its
// OWN `i`, shadowing the hoisted one — if the top-level `i` is never
// READ (only the for's binding is read inside the loop), the hoisted
// declaration is dead weight. The walk is scope-aware and conservative:
// a read inside any scope that re-declares `name` (a nested `let x`, a
// for-init, a closure param/local) does NOT count; any surviving
// unshadowed read keeps the declaration.
pub(crate) fn drop_dead_top_decls(prog: Program) -> Program {
    let mut body = prog.body;
    // the leading top-level declarations and their names
    let mut decl_names: Vec<String> = Vec::new();
    let mut decl_count = 0;
    for st in &body {
        if let Stmt::VariableDeclaration { declarations, .. } = st {
            for d in declarations {
                if let Expr::Identifier { name } = &d.id {
                    decl_names.push(name.clone());
                }
            }
            decl_count += 1;
        } else {
            break; // declarations are leading
        }
    }
    if decl_names.is_empty() {
        return Program { type_: prog.type_, source_type: prog.source_type, body };
    }
    // Scan each leading declaration's name over the statements AFTER that
    // declaration: a `let x` later (nested, or a for-init) shadows the
    // top-level binding for its scope, but the declaration itself is the
    // binding under examination — counting it as a shadow would treat
    // every later `x = …` / `$x` as shadowed. The OTHER leading
    // declarations' initializers DO count as reads (`let middle =
    // [].concat(numbers.slice(…))` reads `numbers`).
    let mut keep_leading: Vec<bool> = Vec::with_capacity(decl_count);
    for (k, st) in body[..decl_count].iter().enumerate() {
        let any_read = match st {
            Stmt::VariableDeclaration { declarations, .. } => declarations.iter().any(|d| {
                matches!(&d.id, Expr::Identifier { name }
                    if stmts_read(&body[k + 1..], name, false))
            }),
            _ => false,
        };
        keep_leading.push(any_read);
    }
    // Drop only the LEADING declarations whose names are all unread — a
    // VariableDeclaration LATER in the body (e.g. the native-array
    // `let arr = […]` placed after an errexit/set -o sequence, or any
    // non-hoist declaration) was never analyzed and must NOT be removed.
    let mut seen = 0usize;
    body.retain(|_| {
        if seen < decl_count {
            let keep = keep_leading[seen];
            seen += 1;
            keep
        } else {
            true
        }
    });
    Program { type_: prog.type_, source_type: prog.source_type, body }
}

fn stmts_read(stmts: &[Stmt], name: &str, shadowed: bool) -> bool {
    let mut sh = shadowed;
    for st in stmts {
        if stmt_read(st, name, sh) {
            return true;
        }
        // a `let x` in this block shadows the remainder of it
        if stmt_declares(st, name) {
            sh = true;
        }
    }
    false
}

fn stmt_read(st: &Stmt, name: &str, shadowed: bool) -> bool {
    match st {
        Stmt::ExpressionStatement { expression } => expr_read(expression, name, shadowed),
        Stmt::BlockStatement { body } => stmts_read(body, name, shadowed),
        Stmt::IfStatement { test, consequent, alternate } => {
            expr_read(test, name, shadowed)
                || stmt_read(consequent, name, shadowed)
                || alternate
                    .as_ref()
                    .map(|a| stmt_read(a, name, shadowed))
                    .unwrap_or(false)
        }
        Stmt::SwitchStatement { discriminant, cases } => {
            expr_read(discriminant, name, shadowed)
                || cases.iter().any(|c| {
                    c.test
                        .as_ref()
                        .map(|t| expr_read(t, name, shadowed))
                        .unwrap_or(false)
                        || stmts_read(&c.consequent, name, shadowed)
                })
        }
        Stmt::WhileStatement { test, body } => {
            expr_read(test, name, shadowed) || stmt_read(body, name, shadowed)
        }
        Stmt::TryStatement {
            block,
            handler,
            finalizer,
        } => {
            stmt_read(block, name, shadowed)
                || handler.as_ref().map(|h| {
                    let param_shadows = h
                        .param
                        .as_ref()
                        .map(|p| expr_read(p, name, false))
                        .unwrap_or(false);
                    stmt_read(&h.body, name, shadowed || param_shadows)
                }).unwrap_or(false)
                || finalizer
                    .as_ref()
                    .map(|f| stmt_read(f, name, shadowed))
                    .unwrap_or(false)
        }
        Stmt::ForStatement { init, test, update, body } => {
            let declares = stmt_declares(init, name);
            stmt_read(init, name, shadowed)
                || expr_read(test, name, shadowed || declares)
                || expr_read(update, name, shadowed || declares)
                || stmt_read(body, name, shadowed || declares)
        }
        Stmt::ForOfStatement { left, right, body } => {
            let declares = stmt_declares(left, name);
            stmt_read(left, name, shadowed)
                || expr_read(right, name, shadowed)
                || stmt_read(body, name, shadowed || declares)
        }
        Stmt::VariableDeclaration { declarations, .. } => declarations.iter().any(|d| match &d.init {
            Some(init) => expr_read(init, name, shadowed),
            None => false,
        }),
        Stmt::ReturnStatement { argument } => argument
            .as_ref()
            .map(|a| expr_read(a, name, shadowed))
            .unwrap_or(false),
        Stmt::BreakStatement { .. } | Stmt::ContinueStatement { .. } => false,
        Stmt::ThrowStatement { argument } => expr_read(argument, name, shadowed),
    }
}

fn stmt_declares(st: &Stmt, name: &str) -> bool {
    match st {
        Stmt::VariableDeclaration { declarations, .. } => declarations.iter().any(|d| {
            matches!(&d.id, Expr::Identifier { name: n } if n == name)
        }),
        Stmt::ForStatement { init, .. } => stmt_declares(init, name),
        Stmt::ForOfStatement { left, .. } => stmt_declares(left, name),
        // The catch param shadows only INSIDE the handler (JS scoping) —
        // it does not declare `name` for the enclosing list, so later
        // statements' reads of a lifted `name` stay visible.
        _ => false,
    }
}

fn expr_read(e: &Expr, name: &str, shadowed: bool) -> bool {
    match e {
        Expr::Identifier { name: n } => !shadowed && n == name,
        Expr::Literal { .. } => false,
        Expr::TemplateLiteral { expressions, .. } => {
            expressions.iter().any(|x| expr_read(x, name, shadowed))
        }
        Expr::CallExpression { callee, arguments, .. } => {
            expr_read(callee, name, shadowed)
                || arguments.iter().any(|a| expr_read(a, name, shadowed))
        }
        Expr::MemberExpression { object, property, .. } => {
            expr_read(object, name, shadowed) || expr_read(property, name, shadowed)
        }
        Expr::AwaitExpression { argument } => expr_read(argument, name, shadowed),
        Expr::ArrowFunctionExpression { params, body, .. } => {
            let p_shadows = params
                .iter()
                .any(|p| matches!(p, Expr::Identifier { name: n } if n == name));
            let b_shadows = arrow_body_declares(body, name);
            arrow_body_read(body, name, shadowed || p_shadows || b_shadows)
        }
        Expr::ObjectExpression { properties } => properties.iter().any(|p| {
            expr_read(&p.value, name, shadowed)
                || (p.computed && expr_read(&p.key, name, shadowed))
        }),
        Expr::ArrayExpression { elements } => {
            elements.iter().flatten().any(|x| expr_read(x, name, shadowed))
        }
        Expr::SpreadElement { argument } => expr_read(argument, name, shadowed),
        Expr::LogicalExpression { left, right, .. } => {
            expr_read(left, name, shadowed) || expr_read(right, name, shadowed)
        }
        Expr::BinaryExpression { left, right, .. } => {
            expr_read(left, name, shadowed) || expr_read(right, name, shadowed)
        }
        Expr::AssignmentExpression { left, right, .. } => {
            expr_read(left, name, shadowed) || expr_read(right, name, shadowed)
        }
        Expr::ConditionalExpression { test, consequent, alternate, .. } => {
            expr_read(test, name, shadowed)
                || expr_read(consequent, name, shadowed)
                || expr_read(alternate, name, shadowed)
        }
        Expr::UnaryExpression { argument, .. } => expr_read(argument, name, shadowed),
        Expr::SequenceExpression { expressions } => {
            expressions.iter().any(|x| expr_read(x, name, shadowed))
        }
        Expr::NewExpression { callee, arguments, .. } => {
            expr_read(callee, name, shadowed)
                || arguments.iter().any(|a| expr_read(a, name, shadowed))
        }
    }
}

fn arrow_body_read(body: &ArrowBody, name: &str, shadowed: bool) -> bool {
    match body {
        ArrowBody::Expr(e) => expr_read(e, name, shadowed),
        ArrowBody::Block(b) => stmt_read(b, name, shadowed),
    }
}

fn arrow_body_declares(body: &ArrowBody, name: &str) -> bool {
    match body {
        ArrowBody::Expr(_) => false,
        ArrowBody::Block(b) => block_declares(b, name),
    }
}

fn block_declares(st: &Stmt, name: &str) -> bool {
    match st {
        Stmt::BlockStatement { body } => body.iter().any(|s| stmt_declares(s, name)),
        Stmt::VariableDeclaration { declarations, .. } => declarations.iter().any(|d| {
            matches!(&d.id, Expr::Identifier { name: n } if n == name)
        }),
        _ => false,
    }
}
