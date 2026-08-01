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

#[derive(Debug, Clone, Serialize)]
#[serde(tag = "type")]
pub enum Expr {
    Identifier {
        name: String,
    },
    Literal {
        value: serde_json::Value,
        raw: Option<String>,
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
    LogicalExpression {
        operator: &'static str,
        left: Box<Expr>,
        right: Box<Expr>,
    },
    BinaryExpression {
        operator: &'static str,
        left: Box<Expr>,
        right: Box<Expr>,
    },
    ConditionalExpression {
        test: Box<Expr>,
        consequent: Box<Expr>,
        alternate: Box<Expr>,
    },
    UnaryExpression {
        operator: &'static str,
        argument: Box<Expr>,
        prefix: bool,
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
        Expr::TemplateLiteral { quasis, expressions } => Expr::TemplateLiteral {
            quasis,
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
    fn echo_lowers_to_exec_call() {
        let json = to_json("echo hello world");
        assert!(json.contains("\"type\":\"Program\""));
        assert!(json.contains("\"name\":\"exec\""));
        assert!(json.contains("hello"));
        assert!(!json.contains("unsupported"));
    }

    #[test]
    fn assignment_lowers_to_setvar() {
        let json = to_json("x=42");
        assert!(json.contains("\"name\":\"setVar\""));
        assert!(json.contains("x"));
        assert!(!json.contains("unsupported"));
    }

    #[test]
    fn if_then_else_lowers_to_if_statement() {
        let json = to_json("if [ -f /tmp/x ]; then echo yes; else echo no; fi");
        assert!(json.contains("\"type\":\"IfStatement\""));
        assert!(json.contains("\"name\":\"test\""));
        assert!(!json.contains("unsupported"));
    }

    #[test]
    fn variable_and_interpolation() {
        let json = to_json("name=world\necho \"Hello $name\"");
        assert!(json.contains("\"type\":\"TemplateLiteral\""));
        assert!(json.contains("\"name\":\"getVar\""));
        assert!(!json.contains("unsupported"));
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
    fn case_lowers_to_switch_statement() {
        let json = to_json("case $x in a) echo a;; *) echo other;; esac");
        assert!(json.contains("\"type\":\"SwitchStatement\""));
        assert!(json.contains("\"type\":\"SwitchCase\""));
        assert!(json.contains("\"name\":\"caseMatch\""));
        assert!(json.contains("\"type\":\"BreakStatement\""));
        assert!(!json.contains("unsupported"));
    }

    #[test]
    fn redirect_lowers_to_redirect_call() {
        let json = to_json("echo hi > out.txt");
        assert!(json.contains("\"name\":\"redirect\""));
        // Property keys serialize as {key: Identifier{name}, value: Literal}.
        assert!(json.contains("\"name\":\"mode\""));
        assert!(json.contains("\"value\":\"w\""));
        assert!(json.contains("\"name\":\"fd\""));
        assert!(json.contains("\"value\":1"));
        assert!(json.contains("\"type\":\"ObjectExpression\""));
        assert!(!json.contains("unsupported"));
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
        assert!(json.contains("\"name\":\"define\""));
        assert!(json.contains("greet"));
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
        assert!(json.contains("\"name\":\"exec\""));
        assert!(json.contains("FOO"));
        assert!(!json.contains("unsupported"));
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
    fn shopt_and_cstyle_for_lower() {
        let json = to_json("shopt -s extglob");
        assert!(json.contains("\"name\":\"shopt\""));
        assert!(!json.contains("unsupported"));
        let json2 = to_json("for ((i=0; i<3; i++)); do echo $i; done");
        assert!(json2.contains("\"name\":\"cstyleFor\""));
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
        // splits it so the parameter expansion is evaluated (sh2.param).
        let json = to_json("X=test; echo --x=\"${X}\"");
        assert!(json.contains("--x="));
        assert!(json.contains("\"name\":\"param\""));
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
        // target (interpolation with getVar("$")) and drops the bogus arg.
        let json = to_json("realpath /bin 2>/tmp/realpath_stderr.$$");
        assert!(json.contains("/tmp/realpath_stderr."));
        assert!(json.contains("\"name\":\"getVar\""));
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
        let json = to_json("cat /tmp/realpath_stderr.$$");
        assert!(json.contains("/tmp/realpath_stderr."));
        assert!(json.contains("\"name\":\"getVar\""));
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
        // Native break inside a switch-case is legal and prevents fallthrough.
        let json = to_json("case $x in a) echo a;; *) echo b;; esac");
        assert!(json.contains("\"type\":\"BreakStatement\""));
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
        // appended for the runtime to turn into a temp file.
        let json = to_json("diff <(echo a) <(echo b)");
        assert!(!json.contains("\"name\":\"unsupported\""));
        assert!(!json.contains("\"value\":\"unsupported\""));
        assert!(json.contains("\"name\":\"capture\""));
        assert!(json.contains("\"name\":\"exec\""));
        // mapfile is stdin-only: no appended path argument, still no gate leak
        let json2 = to_json("mapfile -t lines < <(printf 'x\\ny\\n')");
        assert!(!json2.contains("\"name\":\"unsupported\""));
        assert!(!json2.contains("\"value\":\"unsupported\""));
        assert!(json2.contains("\"name\":\"capture\""));
    }
}
