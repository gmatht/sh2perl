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
        Stmt::ContinueStatement { label } if in_arrow && !in_switch => {
            Stmt::ExpressionStatement {
                expression: sh2_call("continue", vec![]),
            }
        }
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
                fix_stmt(*consequent, in_arrow, in_func, false).unwrap_or(Stmt::BlockStatement {
                    body: vec![],
                }),
            ),
            alternate: alternate.map(|a| {
                Box::new(
                    fix_stmt(*a, in_arrow, in_func, false).unwrap_or(Stmt::BlockStatement {
                        body: vec![],
                    }),
                )
            }),
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
                fix_stmt(*body, in_arrow, in_func, false).unwrap_or(Stmt::BlockStatement {
                    body: vec![],
                }),
            ),
        },
        Stmt::ForOfStatement {
            left,
            right,
            body,
        } => Stmt::ForOfStatement {
            left,
            right: fix_expr(right, in_arrow, in_func),
            body: Box::new(
                fix_stmt(*body, in_arrow, in_func, false).unwrap_or(Stmt::BlockStatement {
                    body: vec![],
                }),
            ),
        },
        Stmt::ForStatement {
            init,
            test,
            update,
            body,
        } => Stmt::ForStatement {
            init: Box::new(
                fix_stmt(*init, in_arrow, in_func, false).unwrap_or(Stmt::BlockStatement {
                    body: vec![],
                }),
            ),
            test: fix_expr(test, in_arrow, in_func),
            update: fix_expr(update, in_arrow, in_func),
            body: Box::new(
                fix_stmt(*body, in_arrow, in_func, false).unwrap_or(Stmt::BlockStatement {
                    body: vec![],
                }),
            ),
        },
        Stmt::VariableDeclaration {
            declarations,
            kind,
        } => Stmt::VariableDeclaration {
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
                ArrowBody::Expr(inner) => ArrowBody::Expr(Box::new(fix_expr(*inner, true, in_func))),
                ArrowBody::Block(b) => ArrowBody::Block(Box::new(
                    fix_stmt(*b, true, in_func, false).unwrap_or(Stmt::BlockStatement {
                        body: vec![],
                    }),
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
        Expr::TemplateLiteral { quasis, expressions } => Expr::TemplateLiteral {
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
        Some("mapfile") | Some("readarray") | Some("head") | Some("tail") | Some("cat") | Some("wc")
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
        Command::And(l, r) => Command::And(
            Box::new(transform_cmd(l)),
            Box::new(transform_cmd(r)),
        ),
        Command::Or(l, r) => Command::Or(
            Box::new(transform_cmd(l)),
            Box::new(transform_cmd(r)),
        ),
        Command::Not(c) => Command::Not(Box::new(transform_cmd(c))),
        Command::Background(c) => Command::Background(Box::new(transform_cmd(c))),
        Command::Subshell(c) => Command::Subshell(Box::new(transform_cmd(c))),
        Command::If(i) => {
            let mut i = i.clone();
            i.condition = Box::new(transform_cmd(&i.condition));
            i.then_branch = Box::new(transform_cmd(&i.then_branch));
            i.else_branch = i
                .else_branch
                .map(|b| Box::new(transform_cmd(&b)));
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
            if interp.parts.iter().all(|p| matches!(p, StringPart::Literal(_))) =>
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
            raw: r.clone(),
            cooked: Some(r),
        },
        tail,
    }
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
        // Plan 4 liveness now marks it dead when nothing reads the if's
        // status — the if lowers to a plain `if (c) { ... }`, no else.
        let json = to_json("if false; then echo yes; fi");
        assert!(json.contains("\"type\":\"IfStatement\""));
        assert!(json.contains("\"alternate\":null"), "dead status write → no else");
        assert!(!json.contains("unsupported"));
        // a READER keeps the write: `; echo $?` observes the false-path 0
        let json2 = to_json("if false; then echo yes; fi; echo $?");
        assert!(json2.contains("\"alternate\":{\"type\""), "read status → else kept");
        assert!(!json2.contains("unsupported"));
        // a later WRITER shadows the if's status → the write is dead again
        let json3 = to_json("if false; then echo yes; fi; false; echo $?");
        assert!(json3.contains("\"alternate\":null"), "shadowed by `false` → no else");
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
        // A1 split marker) and legitimately takes the flat/join path (the
        // shortcut would comma-join a multi-word value).
        let json = to_json("i=42; echo \"$i\"");
        assert!(json.contains("\"name\":\"String\""));
        assert!(!json.contains("\"name\":\"join\""), "single arg: no join");
        assert!(!json.contains("\"type\":\"ArrayExpression\""), "single arg: no array");
        assert!(!json.contains("unsupported"));
        // unquoted: the split arg keeps the flat/join path
        let json_unq = to_json("i=42; echo $i");
        assert!(json_unq.contains("\"name\":\"join\""));
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
        assert!(!json4.contains("\"name\":\"join\""), "capture-assigned var is a scalar");
        assert!(!json4.contains("unsupported"));
    }

    #[test]
    fn grep_null_test_lifts_to_contains() {
        // `if echo $x | grep P >/dev/null 2>/dev/null` (discarded-output grep
        // as a test) is a substring test — no echo/grep spawns, no pipeline;
        // the emitter inlines the ShIR `contains` call to a NATIVE
        // `String(h).includes(n)` (src/shir.rs expr_to_estree).
        let json = to_json(
            "if echo hi | grep hi > /dev/null 2> /dev/null; then echo yes; fi",
        );
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
        // pipeline + builtin
        let json = to_json("x=$(echo a:b:c | cut -d: -f$n)");
        assert!(json.contains("\"name\":\"pipeline\""));
        assert!(!json.contains("\"name\":\"slice\""));
        assert!(!json.contains("\"name\":\"filter\""));
        assert!(!json.contains("unsupported"));
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
        let json = to_json(
            "if [[ \"$a\" == \"x\" ]] && [[ \"$b\" == \"y\" ]]; then echo yes; fi",
        );
        // the sh2.test DISPATCH is gone (a regex-literal `.test()` method
        // call has a different callee shape and may legitimately appear)
        assert!(!json.contains("\"name\":\"sh2\"},\"property\":{\"type\":\"Identifier\",\"name\":\"test\""));
        assert!(json.contains("\"name\":\"getVar\""));
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
        assert!(!json.contains("\"name\":\"sh2\"},\"property\":{\"type\":\"Identifier\",\"name\":\"test\""));
        assert!(json.contains("\"name\":\"positional\""));
        assert!(json.contains("\"operator\":\"||\""));
        assert!(!json.contains("unsupported"));
    }

    #[test]
    fn status_equality_lowers_to_lastexit_read() {
        // `[ "$?" = "0" ]` — the `$?` sigil is a status-field read, not
        // a glob `?`: `String(sh2.lastExit) === "0"`, zero dispatches.
        let json = to_json("if [ \"$?\" = \"0\" ]; then echo zero; fi");
        assert!(!json.contains("\"name\":\"sh2\"},\"property\":{\"type\":\"Identifier\",\"name\":\"test\""));
        assert!(json.contains("\"name\":\"lastExit\""));
        assert!(json.contains("\"name\":\"String\""));
        assert!(!json.contains("unsupported"));
    }

    #[test]
    fn grep_with_regex_pattern_not_lifted() {
        // BRE metacharacters disqualify the lift: `grep 'a.c'` is a regex,
        // not a substring test — the pipeline must stay.
        let json = to_json(
            "if echo hi | grep a.c > /dev/null 2> /dev/null; then echo yes; fi",
        );
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
        // a batch_ok loop with an AWAITING body (a capture assign inside)
        // never takes the batch path — the sync bodyFn cannot await
        let json3 = to_json("for i in 1 2 3; do x=$(ls); done");
        assert!(!json3.contains("\"name\":\"forLoopBatch\""));
        assert!(!json3.contains("\"name\":\"forLoopSync\""));
        assert!(!json3.contains("\"type\":\"ForOfStatement\""));
        assert!(json3.contains("\"name\":\"forLoop\""));
        assert!(!json3.contains("unsupported"));
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
        assert!(!json2.contains("\"name\":\"Number\""), "no (Number(i) || 0) coercion");
        assert!(!json2.contains("\"name\":\"captureWords\""));
        assert!(!json2.contains("unsupported"));
    }

    #[test]
    fn seq_range_for_conservative_cases() {
        // 3-arg step forms (`seq A S B`) keep the runtime path
        let json = to_json("for i in $(seq 1 2 10); do echo $i; done");
        assert!(json.contains("\"name\":\"captureWords\""));
        assert!(!json.contains("\"type\":\"ForStatement\""));
        // leading-zero args (`seq 01 10` — GNU pads, bash arith is octal)
        let json2 = to_json("for i in $(seq 01 10); do echo $i; done");
        assert!(json2.contains("\"name\":\"captureWords\""));
        assert!(!json2.contains("\"type\":\"ForStatement\""));
        // a body WRITE to the loop var keeps word-list semantics (a
        // counter's i++ would read the body-written value)
        let json3 = to_json("for i in $(seq 1 3); do i=99; echo $i; done");
        assert!(json3.contains("\"name\":\"captureWords\""));
        assert!(!json3.contains("\"type\":\"ForStatement\""));
        // a nested loop binding the SAME var keeps the OUTER on the word
        // path (bash clobbers i in the body; a counter's i++ would read
        // the body-written value). The INNER loop — whose own body never
        // writes its var — still transforms independently.
        let json4 = to_json(
            "for i in $(seq 1 2); do for i in $(seq 10 12); do echo $i; done; done",
        );
        assert!(json4.contains("\"name\":\"captureWords\""), "outer keeps the word list");
        assert!(json4.contains("\"type\":\"ForOfStatement\""), "outer is a word loop");
        assert!(json4.contains("\"type\":\"ForStatement\""), "inner is a counter loop");
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
        assert!(json4.contains("\"name\":\"capture\""));
        assert!(!json4.contains("\"name\":\"setVar\""));
    }

    #[test]
    fn if_then_else_lowers_to_if_statement() {
        // `[ -f /tmp/x ]` is a file test — a native async lstat chain
        // (no sh2.test string parse, no dispatch, no blocking lstatSync).
        let json = to_json("if [ -f /tmp/x ]; then echo yes; else echo no; fi");
        assert!(json.contains("\"type\":\"IfStatement\""));
        assert!(json.contains("\"name\":\"lstat\""));
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
        assert!(!json2.contains("\"name\":\"join\""), "single interpolated arg: no join");
        assert!(json2.contains("\"type\":\"TemplateLiteral\""));
        assert!(!json2.contains("unsupported"));
        let json3 = to_json("read name\necho \"Hello $name\"");
        assert!(json3.contains("\"name\":\"getVar\""));
        assert!(!json3.contains("unsupported"));
    }

    #[test]
    fn pipeline_lowers_to_pipeline_call() {
        let json = to_json("ls | grep foo");
        assert!(json.contains("\"name\":\"pipeline\""));
        assert!(json.contains("\"type\":\"ArrowFunctionExpression\""));
        assert!(!json.contains("unsupported"));
    }

    #[test]
    fn command_substitution_uses_await_capture() {
        // Unquoted $(...) word-splits: captureWords returns an arg array.
        let json = to_json("echo $(date)");
        assert!(json.contains("\"type\":\"AwaitExpression\""));
        assert!(json.contains("\"name\":\"captureWords\""));
        assert!(!json.contains("unsupported"));
        // Quoted "$(...)" stays a plain template capture (no word splitting).
        let json2 = to_json("echo \"$(date)\"");
        assert!(json2.contains("\"name\":\"capture\""));
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
        assert!(json.contains("\"value\":\"5\\n\""), "static bc program folds");
        assert!(!json.contains("\"name\":\"capture\""));
        assert!(!json.contains("\"name\":\"pipeline\""));
        assert!(!json.contains("\"name\":\"exec\""));
        assert!(!json.contains("unsupported"));
        // the runtime-var sqrt form (store-bound $n → sh2.getVar)
        let json2 = to_json("read n; echo \"$(echo \"sqrt($n)\" | bc)\"");
        assert!(json2.contains("\"name\":\"sqrt\""), "native sqrt expr");
        assert!(json2.contains("\"name\":\"floor\""));
        assert!(json2.contains("\"name\":\"getVar\""));
        assert!(!json2.contains("\"name\":\"pipeline\""));
        assert!(!json2.contains("\"name\":\"capture\""));
        assert!(!json2.contains("\"name\":\"exec\""));
        assert!(!json2.contains("unsupported"));
        // the general var-operand form (`$sum + $i` — the in-loop bc
        // capture): native `String(Number(sum) + Number(i))`, no spawn
        let json3 = to_json("sum=0; for i in 1 2 3; do sum=$(echo \"$sum + $i\" | bc); done; echo $sum");
        assert!(json3.contains("\"name\":\"Number\""), "native var arith");
        assert!(json3.contains("\"operator\":\"+\""));
        assert!(!json3.contains("\"name\":\"pipeline\""));
        assert!(!json3.contains("\"name\":\"capture\""));
        assert!(!json3.contains("\"name\":\"exec\""));
        assert!(!json3.contains("\"name\":\"forLoop\""), "the loop goes native for-of");
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
        assert!(json.contains("\"name\":\"includes\"")
            || json.contains("\"operator\":\"===\""));
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
        // non-echo bodies keep the runtime redirect
        let json2 = to_json("ls > out.txt");
        assert!(json2.contains("\"name\":\"redirect\""));
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
        let json = to_json("cat << 'EOF'\nhi there\nEOF");
        assert!(json.contains("\"value\":\"heredoc\""));
        assert!(json.contains("hi there"));
        assert!(json.contains("\"value\":false"));
        assert!(!json.contains("unsupported"));
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
        let json = to_json("(echo hi)");
        assert!(json.contains("\"name\":\"subshell\""));
        assert!(!json.contains("unsupported"));
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
        let json3 = to_json("FOO=bar ls x");
        assert!(json3.contains("\"name\":\"exec\""));
        assert!(json3.contains("FOO"));
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
        // desync from the store)
        let json3 = to_json("typeset -i i\ni=foo\n((i++))");
        assert!(json3.contains("\"name\":\"setVar\""));
        assert!(json3.contains("\"name\":\"getVar\""));
        // the same lift works through `let` statements without any declare
        let json4 = to_json("((i++))");
        assert!(!json4.contains("\"name\":\"setVar\""));
        assert!(!json4.contains("\"name\":\"getVar\""));
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
        // `echo "${var:?unset"` (missing closing `}`) is a bash parse error:
        // the lexer artifact word must never print. The command lowers to
        // nothing (BlankLine) — an empty program, matching bash's abort.
        let json = to_json("echo \"${var:?unset\"");
        assert!(!json.contains("unset"));
        assert!(!json.contains("\"name\":\"exec\""));
        assert!(!json.contains("\"name\":\"unsupported\""));
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
        assert!(json.contains("\"name\":\"exec\""));
        // mapfile is stdin-only: no appended path argument, still no gate leak
        // (the producer's capture is lowered as a here-string fd-0 redirect
        // feeding the sync mapfile builtin — no async capture machinery).
        let json2 = to_json("mapfile -t lines < <(printf 'x\\ny\\n')");
        assert!(!json2.contains("\"name\":\"unsupported\""));
        assert!(!json2.contains("\"value\":\"unsupported\""));
        assert!(json2.contains("\"name\":\"redirect\""));
        assert!(json2.contains("\"name\":\"builtin\""));
    }
}
