//! ShIR — the language-neutral layer between the shell AST and the backends.
//!
//! `ast_to_ir` builds an `IrProgram` from the parsed shell AST using neutral
//! IR nodes (plus `sh2.*`-namespace calls expressed via `IrExpr::Call`); the
//! ESTree emitter consumes this IR via `shir_to_estree`, so the shell→ESTree
//! lowering logic lives in one place (PLAN.md §3). The Perl generator builds
//! its own IR flavor for `ir_to_perl`; the neutral nodes here
//! (Case/Redirect/Function/Subshell/Background/Arrow/...) are ESTree-path only.

use crate::ast::*;
use crate::estree::*;
use crate::ir::*;

fn call(func: &str, args: Vec<IrExpr>) -> IrExpr {
    IrExpr::Call {
        func: func.to_string(),
        args,
    }
}

fn st(s: &str) -> IrExpr {
    IrExpr::Str(s.to_string(), StrStyle::DoubleQuoted)
}

// ── AST → IR ─────────────────────────────────────────────────────────

pub fn ast_to_ir(commands: &[Command]) -> IrProgram {
    // Shared optimization passes (M6): the same optimize_stmts the Perl
    // backend runs now also runs here, so future passes (constant folding,
    // dead-assignment elimination) benefit both consumers of the IR.
    let stmts = crate::ir::optimize_stmts(&commands.iter().filter_map(stmt_for_command).collect::<Vec<_>>());
    IrProgram {
        imports: vec![],
        requires: vec![],
        stmts,
        subs: vec![],
    }
}

fn stmt_for_command(cmd: &Command) -> Option<IrStmt> {
    Some(match cmd {
        Command::BlankLine => return None,
        Command::TestExpression(t) => {
            IrStmt::Expr(call("test", vec![st(&t.expression)]))
        }
        Command::Simple(sc) => exec_stmt(&sc.name, &sc.args, &sc.env_vars, &sc.redirects),
        Command::BuiltinCommand(bc) => exec_stmt(
            &Word::Literal(bc.name.clone(), None),
            &bc.args,
            &bc.env_vars,
            &bc.redirects,
        ),
        Command::Assignment(a) => IrStmt::Assign {
            targets: vec![AssignTarget {
                var: a.variable.clone(),
                sigil: None,
                indices: vec![],
            }],
            expr: assignment_value_ir(a),
        },
        Command::If(if_stmt) => IrStmt::If {
            cond: command_to_ir(&if_stmt.condition),
            then: body_stmts(&if_stmt.then_branch),
            elsifs: vec![],
            else_: if_stmt
                .else_branch
                .as_ref()
                .map(|b| body_stmts(b.as_ref()))
                .unwrap_or_default(),
        },
        Command::Case(c) => case_to_ir(c),
        Command::While(w) => {
            let cond = command_to_ir(&w.condition);
            IrStmt::While {
                cond: if w.is_until {
                    not_ir(cond)
                } else {
                    cond
                },
                body: body_stmts(&Command::Block(w.body.clone())),
            }
        }
        Command::For(f) => IrStmt::For {
            var: f.variable.clone(),
            iter: IrExpr::Array(for_items_ir(&f.items)),
            body: body_stmts(&Command::Block(f.body.clone())),
        },
        Command::Block(b) => IrStmt::Block(
            b.commands.iter().filter_map(stmt_for_command).collect(),
        ),
        Command::Pipeline(p) => IrStmt::Expr(call(
            "pipeline",
            vec![IrExpr::Array(
                p.commands
                    .iter()
                    .map(|c| IrExpr::Arrow(command_arrow_stmts(c)))
                    .collect(),
            )],
        )),
        Command::ShoptCommand(s) => {
            IrStmt::Expr(call("shopt", vec![st(&s.option), IrExpr::Bool(s.enable)]))
        }
        Command::CStyleFor(cf) => IrStmt::Expr(call(
            "cstyleFor",
            vec![
                st(&cf.arith_content),
                IrExpr::Arrow(body_stmts(&Command::Block(cf.body.clone()))),
            ],
        )),
        Command::Function(f) => IrStmt::Function {
            name: f.name.clone(),
            body: body_stmts(&Command::Block(f.body.clone())),
        },
        Command::Subshell(c) => IrStmt::Subshell(command_arrow_stmts(c)),
        Command::Background(c) => IrStmt::Background(command_arrow_stmts(c)),
        Command::Redirect(rc) => IrStmt::Redirect {
            inner: vec![stmt_for_command(&rc.command).unwrap_or(IrStmt::Expr(call("true", vec![])))],
            redirects: rc.redirects.iter().map(redirect_to_ir).collect(),
        },
        Command::And(l, r) => IrStmt::Expr(IrExpr::BinOp {
            op: BinOpKind::And,
            lhs: Box::new(command_to_ir(l)),
            rhs: Box::new(command_to_ir(r)),
        }),
        Command::Or(l, r) => IrStmt::Expr(IrExpr::BinOp {
            op: BinOpKind::Or,
            lhs: Box::new(command_to_ir(l)),
            rhs: Box::new(command_to_ir(r)),
        }),
        Command::Not(c) => IrStmt::Expr(not_ir(command_to_ir(c))),
        Command::Break(_) => IrStmt::Expr(call("break", vec![])),
        Command::Continue(_) => IrStmt::Expr(call("continue", vec![])),
        Command::Return(w) => IrStmt::Return(w.as_ref().map(word_ir_quoted)),
        other => IrStmt::Expr(call(
            "unsupported",
            vec![st(&format!("{other:?}"))],
        )),
    })
}

fn not_ir(inner: IrExpr) -> IrExpr {
    IrExpr::BinOp {
        op: BinOpKind::Not,
        lhs: Box::new(inner.clone()),
        rhs: Box::new(inner),
    }
}

fn command_arrow_stmts(c: &Command) -> Vec<IrStmt> {
    match c {
        Command::Block(b) => b.commands.iter().filter_map(stmt_for_command).collect(),
        // expression-bodied arrows
        Command::Simple(_)
        | Command::BuiltinCommand(_)
        | Command::TestExpression(_)
        | Command::Redirect(_)
        | Command::Pipeline(_)
        | Command::And(_, _)
        | Command::Or(_, _)
        | Command::Not(_)
        | Command::Assignment(_)
        | Command::ShoptCommand(_) => vec![IrStmt::Expr(command_to_ir(c))],
        // compound commands → block-bodied arrows
        other => vec![stmt_for_command(other).unwrap_or(IrStmt::Expr(call("true", vec![])))],
    }
}

fn body_stmts(cmd: &Command) -> Vec<IrStmt> {
    match cmd {
        Command::Block(b) => b.commands.iter().filter_map(stmt_for_command).collect(),
        _ => stmt_for_command(cmd).map(|s| vec![s]).unwrap_or_default(),
    }
}

fn exec_stmt(
    name: &Word,
    args: &[Word],
    env: &std::collections::BTreeMap<String, Word>,
    redirects: &[Redirect],
) -> IrStmt {
    let exec_call = exec_call_ir(name, args, env);
    if redirects.is_empty() {
        IrStmt::Expr(exec_call)
    } else {
        IrStmt::Redirect {
            inner: vec![IrStmt::Expr(exec_call)],
            redirects: redirects.iter().map(redirect_to_ir).collect(),
        }
    }
}

fn exec_call_ir(
    name: &Word,
    args: &[Word],
    env: &std::collections::BTreeMap<String, Word>,
) -> IrExpr {
    // `declare -A map=(...)` / `local -A options=()` — the array-literal arg
    // is lowered to a side-effecting setArray call; tell it the map is
    // associative so it registers the assoc store.
    let assoc = args.iter().any(|a| matches!(a, Word::Literal(s, _) if s.starts_with("-A")));
    let mut call_args = vec![word_ir(name), IrExpr::Array(exec_args_ir(args, assoc))];
    if !env.is_empty() {
        call_args.push(IrExpr::Object(
            env.iter()
                .map(|(k, v)| (k.clone(), word_ir_quoted(v)))
                .collect(),
        ));
    }
    call("exec", call_args)
}

/// Marker prefix the runtime recognizes on exec args / for-loop items (see
/// sh2-namespace.mjs: `exec` glob-expands the suffix against the filesystem).
/// Only UNQUOTED words may glob; the parser keeps double-quoted words as
/// StringInterpolation (never tagged here), and single-quoted words are
/// indistinguishable from bare ones in the AST — the corpus has no
/// single-quoted globs in exec-arg position, so tagging all Literals with
/// glob chars matches bash for every example.
const GLOB_MAGIC: &str = "\u{1}SH2GLOB\u{1}";

fn has_glob_chars(s: &str) -> bool {
    // Skip `${...}` regions: `[`/`*`/`?` inside an expansion (`${#x[@]}`, `${x:-*}`)
    // are parameter syntax, not glob patterns.
    let bytes = s.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'$' && i + 1 < bytes.len() && bytes[i + 1] == b'{' {
            let mut depth = 1usize;
            let mut j = i + 2;
            while j < bytes.len() && depth > 0 {
                if bytes[j] == b'{' {
                    depth += 1;
                } else if bytes[j] == b'}' {
                    depth -= 1;
                }
                j += 1;
            }
            i = j;
            continue;
        }
        if bytes[i] == b'*' || bytes[i] == b'?' || bytes[i] == b'[' {
            return true;
        }
        i += 1;
    }
    false
}

/// Exec-argument lowering: merges consecutive brace expansions into a single
/// cross-product `sh2.brace` call (`{a,b}{1,2}` is ONE bash word — the
/// parser splits it into two words, so the emitter re-joins them, exactly
/// like the perl backend's cartesian-product pass), and tags unquoted glob
/// words for the runtime to expand.
fn exec_args_ir(args: &[Word], assoc: bool) -> Vec<IrExpr> {
    merged_words_ir(args, &|w| arg_word_ir(w, assoc))
}

/// Same brace merge for for-loop item lists (`for x in {a,b}{1,2}`); the
/// single-word fallback keeps the `$@`/`$*` listVar special case.
fn for_items_ir(items: &[Word]) -> Vec<IrExpr> {
    merged_words_ir(items, &for_item_ir)
}

fn merged_words_ir(words: &[Word], single: &dyn Fn(&Word) -> IrExpr) -> Vec<IrExpr> {
    let mut out: Vec<IrExpr> = Vec::new();
    let mut i = 0;
    while i < words.len() {
        if let Word::BraceExpansion(be, _) = &words[i] {
            let mut groups = vec![brace_items_json(&be.items)];
            let mut middles: Vec<serde_json::Value> = Vec::new();
            let mut suffix = be.suffix.clone().unwrap_or_default();
            let prefix = be.prefix.clone().unwrap_or_default();
            i += 1;
            while i < words.len() {
                if let Word::BraceExpansion(be2, _) = &words[i] {
                    middles.push(serde_json::Value::String(format!(
                        "{}{}",
                        suffix,
                        be2.prefix.as_deref().unwrap_or("")
                    )));
                    groups.push(brace_items_json(&be2.items));
                    suffix = be2.suffix.clone().unwrap_or_default();
                    i += 1;
                } else {
                    break;
                }
            }
            // A brace expansion whose prefix/suffix/items contain glob chars
            // must glob EACH result (`*.{txt,log}` → `*.txt` → files): the
            // runtime globs any result string that starts with GLOB_MAGIC.
            let magic_prefix = if has_glob_chars(&prefix)
                || has_glob_chars(&suffix)
                || be_items_contain_glob(&groups)
            {
                format!("{GLOB_MAGIC}{prefix}")
            } else {
                prefix.clone()
            };
            out.push(call(
                "brace",
                vec![
                    st(&magic_prefix),
                    IrExpr::Json(serde_json::Value::Array(groups)),
                    IrExpr::Json(serde_json::Value::Array(middles)),
                    st(&suffix),
                ],
            ));
        } else {
            out.push(single(&words[i]));
            i += 1;
        }
    }
    out
}

fn be_items_contain_glob(groups: &[serde_json::Value]) -> bool {
    fn collect(v: &serde_json::Value, found: &mut bool) {
        match v {
            serde_json::Value::String(s) => {
                if has_glob_chars(s) {
                    *found = true;
                }
            }
            serde_json::Value::Array(a) => a.iter().for_each(|x| collect(x, found)),
            serde_json::Value::Object(o) => o.values().for_each(|x| collect(x, found)),
            _ => {}
        }
    }
    let mut found = false;
    for g in groups {
        collect(g, &mut found);
    }
    found
}

/// A single exec-argument word (non-brace). Unquoted glob words get the
/// GLOB_MAGIC tag so the runtime expands them against the filesystem. An
/// array literal (`declare -a arr=(a b)`) lowers to a side-effecting
/// setArray call whose (magic) return value is dropped by the runtime's exec
/// arg flattener; `assoc` marks `declare -A` literals.
fn arg_word_ir(w: &Word, assoc: bool) -> IrExpr {
    match w {
        // Quote removal FIRST, then tag for globbing: an unquoted `\*` is a
        // literal `*` after removal (never a glob), while `*.txt` globs.
        Word::Literal(s, ann) => {
            let s2 = shell_quote_removal(s);
            // A single-quoted word (`'*.txt'`) is LITERAL — bash never globs
            // it. The parser marks quoted words (ann == Some); without the
            // marker the AST cannot distinguish `'*.txt'` from `*.txt`.
            if ann.is_none() && has_glob_chars(&s2) {
                st(&format!("{GLOB_MAGIC}{s2}"))
            } else {
                st(&s2)
            }
        }
        Word::Array(name, elements, _) => call(
            "setArray",
            vec![
                st(name),
                IrExpr::Array(elements.iter().map(|e| st(e)).collect()),
                IrExpr::Bool(assoc),
            ],
        ),
        _ => word_ir(w),
    }
}

/// `name op value` — the RHS expression for a statement-level assignment
/// (`IrStmt::Assign` wraps it in `sh2.setVar`). Compound operators lower to
/// `sh2.assign` (which sets the variable itself), array `+=` to
/// `sh2.setArrayAppend`.
fn assignment_value_ir(a: &Assignment) -> IrExpr {
    match &a.value {
        Word::Array(name, elements, _) if a.operator == AssignmentOperator::PlusAssign => call(
            "setArrayAppend",
            vec![st(name), IrExpr::Array(elements.iter().map(|e| st(e)).collect())],
        ),
        Word::Array(name, elements, _) => call(
            "setArray",
            vec![st(name), IrExpr::Array(elements.iter().map(|e| st(e)).collect())],
        ),
        _ if a.operator == AssignmentOperator::Assign => word_ir_quoted(&a.value),
        _ => call(
            "assign",
            vec![
                st(&a.variable),
                st(assign_op_str(&a.operator)),
                word_ir_quoted(&a.value),
            ],
        ),
    }
}

/// `name op value` in EXPRESSION context (`&&`/`||` operands, `if`/`while`
/// conditions): the assignment must still happen AND the expression must be
/// truthy. All three helpers return true.
fn assignment_expr_ir(a: &Assignment) -> IrExpr {
    match &a.value {
        Word::Array(name, elements, _) if a.operator == AssignmentOperator::PlusAssign => call(
            "setArrayAppend",
            vec![st(name), IrExpr::Array(elements.iter().map(|e| st(e)).collect())],
        ),
        Word::Array(name, elements, _) => call(
            "setArray",
            vec![st(name), IrExpr::Array(elements.iter().map(|e| st(e)).collect())],
        ),
        _ => call(
            "assign",
            vec![
                st(&a.variable),
                st(assign_op_str(&a.operator)),
                word_ir_quoted(&a.value),
            ],
        ),
    }
}

fn assign_op_str(op: &AssignmentOperator) -> &'static str {
    match op {
        AssignmentOperator::Assign => "=",
        AssignmentOperator::PlusAssign => "+=",
        AssignmentOperator::MinusAssign => "-=",
        AssignmentOperator::StarAssign => "*=",
        AssignmentOperator::SlashAssign => "/=",
        AssignmentOperator::PercentAssign => "%=",
    }
}

fn redirect_to_ir(r: &Redirect) -> IrRedirect {
    // `2>&1` / `>&2` / `<&0` — the parser stores the dup TARGET without the
    // `&` (Literal("1")), indistinguishable from `2> 1`. The perl generator
    // resolves the ambiguity in favor of a dup for all-digit targets on
    // stderr/input operators; do the same here and re-attach the `&` so the
    // runtime's dup branch (`target` matching `&N`) handles it.
    let digit_target = matches!(
        &r.target,
        Word::Literal(s, _) if !s.is_empty() && s.chars().all(|c| c.is_ascii_digit())
    );
    let (mode, default_fd) = match &r.operator {
        RedirectOperator::Input => ("r", 0),
        RedirectOperator::Output => ("w", 1),
        RedirectOperator::Append => ("a", 1),
        RedirectOperator::InputOutput => ("r+", 0),
        RedirectOperator::Heredoc => ("heredoc", 0),
        RedirectOperator::HeredocTabs => ("heredoc-tabs", 0),
        RedirectOperator::HereString => ("herestring", 0),
        RedirectOperator::StderrOutput => ("w", 2),
        RedirectOperator::StderrAppend => ("a", 2),
        RedirectOperator::StderrInput => ("r", 2),
        RedirectOperator::ProcessSubstitutionInput(_) => ("unsupported", 0),
        RedirectOperator::ProcessSubstitutionOutput(_) => ("unsupported", 0),
    };
    let is_dup = digit_target
        && matches!(
            r.operator,
            RedirectOperator::Input
                | RedirectOperator::StderrOutput
                | RedirectOperator::StderrAppend
                | RedirectOperator::StderrInput
        );
    let fd = if is_dup {
        // `>&2` has fd None (default stdout); `2>&1` carries fd 2 explicitly.
        r.fd.or(Some(1))
    } else {
        r.fd.or(Some(default_fd))
    };
    let target = if is_dup {
        match &r.target {
            Word::Literal(s, _) => st(&format!("&{s}")),
            _ => unreachable!(),
        }
    } else {
        match &r.operator {
            RedirectOperator::Heredoc | RedirectOperator::HeredocTabs => {
                st(r.heredoc_body.as_deref().unwrap_or(""))
            }
            _ => word_ir(&r.target),
        }
    };
    IrRedirect {
        fd,
        mode: mode.to_string(),
        target,
        interpolate: !r.heredoc_quoted,
    }
}

fn case_to_ir(c: &CaseStatement) -> IrStmt {
    IrStmt::Case {
        discriminant: word_ir(&c.word),
        clauses: c
            .cases
            .iter()
            .map(|cl| IrCaseClause {
                patterns: cl.patterns.iter().map(|p| p.to_string()).collect(),
                body: cl.body.iter().filter_map(stmt_for_command).collect(),
            })
            .collect(),
    }
}

fn command_to_ir(cmd: &Command) -> IrExpr {
    match cmd {
        Command::TestExpression(t) => call("test", vec![st(&t.expression)]),
        Command::Simple(sc) => exec_expr(&sc.name, &sc.args, &sc.env_vars, &sc.redirects),
        Command::BuiltinCommand(bc) => exec_expr(
            &Word::Literal(bc.name.clone(), None),
            &bc.args,
            &bc.env_vars,
            &bc.redirects,
        ),
        Command::Redirect(rc) => {
            // `exec 4>&1` in expression context: bash installs the redirects
            // permanently in the shell's fd table (see stmt_to_estree's
            // IrStmt::Redirect persist rule — same qualification here).
            let persist = is_bare_exec(&rc.command);
            call(
                "redirect",
                vec![
                    IrExpr::Arrow(vec![IrStmt::Expr(command_to_ir(&rc.command))]),
                    IrExpr::Array(
                        rc.redirects
                            .iter()
                            .map(|r| redirect_spec_object_persist(r, persist))
                            .collect(),
                    ),
                ],
            )
        }
        Command::Pipeline(p) => call(
            "pipeline",
            vec![IrExpr::Array(
                p.commands
                    .iter()
                    .map(|c| IrExpr::Arrow(command_arrow_stmts(c)))
                    .collect(),
            )],
        ),
        Command::Subshell(c) => call("subshell", vec![IrExpr::Arrow(command_arrow_stmts(c))]),
        Command::Block(b) => call(
            "block",
            vec![IrExpr::Arrow(
                b.commands.iter().filter_map(stmt_for_command).collect(),
            )],
        ),
        Command::While(w) => call(
            "whileLoop",
            vec![
                IrExpr::Arrow(vec![IrStmt::Expr(if w.is_until {
                    not_ir(command_to_ir(&w.condition))
                } else {
                    command_to_ir(&w.condition)
                })]),
                IrExpr::Arrow(body_stmts(&Command::Block(w.body.clone()))),
            ],
        ),
        Command::Assignment(a) => assignment_expr_ir(a),
        Command::ShoptCommand(s) => call("shopt", vec![st(&s.option), IrExpr::Bool(s.enable)]),
        Command::And(l, r) => IrExpr::BinOp {
            op: BinOpKind::And,
            lhs: Box::new(command_to_ir(l)),
            rhs: Box::new(command_to_ir(r)),
        },
        Command::Or(l, r) => IrExpr::BinOp {
            op: BinOpKind::Or,
            lhs: Box::new(command_to_ir(l)),
            rhs: Box::new(command_to_ir(r)),
        },
        Command::Not(c) => not_ir(command_to_ir(c)),
        Command::Return(w) => {
            let mut args = vec![];
            if let Some(w) = w {
                args.push(word_ir_quoted(w));
            }
            call("return", args)
        }
        Command::Break(_) => call("break", vec![]),
        Command::Continue(_) => call("continue", vec![]),
        other => call("unsupported", vec![st(&format!("{other:?}"))]),
    }
}

/// The literal `exec` builtin with NO args (a redirect-only `exec N>&M`):
/// bash installs those redirects permanently in the shell's own fd table.
fn is_bare_exec(cmd: &Command) -> bool {
    match cmd {
        Command::BuiltinCommand(bc) => bc.name == "exec" && bc.args.is_empty(),
        Command::Simple(sc) => {
            sc.name.as_literal().map_or(false, |n| n == "exec") && sc.args.is_empty()
        }
        _ => false,
    }
}

fn exec_expr(
    name: &Word,
    args: &[Word],
    env: &std::collections::BTreeMap<String, Word>,
    redirects: &[Redirect],
) -> IrExpr {
    let exec_call = exec_call_ir(name, args, env);
    if redirects.is_empty() {
        exec_call
    } else {
        // `exec 4>&1` in EXPRESSION context (command substitution bodies,
        // if/while conditions, pipeline stages): bash installs the redirects
        // permanently in the shell's own fd table, so the runtime must keep
        // them after the redirect call (same rule as the statement path in
        // stmt_to_estree — only the literal `exec` builtin with no args).
        let persist = matches!(name, Word::Literal(s, _) if s == "exec") && args.is_empty();
        call(
            "redirect",
            vec![
                IrExpr::Arrow(vec![IrStmt::Expr(exec_call)]),
                IrExpr::Array(
                    redirects
                        .iter()
                        .map(|r| redirect_spec_object_persist(r, persist))
                        .collect(),
                ),
            ],
        )
    }
}

fn redirect_spec_object(r: &Redirect) -> IrExpr {
    redirect_spec_object_persist(r, false)
}

fn redirect_spec_object_persist(r: &Redirect, persist: bool) -> IrExpr {
    let ir = redirect_to_ir(r);
    let mut props = vec![
        ("fd".to_string(), IrExpr::Int(ir.fd.unwrap_or(0) as i64)),
        ("mode".to_string(), st(&ir.mode)),
        ("target".to_string(), ir.target),
    ];
    if ir.mode == "heredoc" || ir.mode == "heredoc-tabs" {
        props.push(("interpolate".to_string(), IrExpr::Bool(ir.interpolate)));
    }
    if persist {
        props.push(("persist".to_string(), IrExpr::Bool(true)));
    }
    IrExpr::Object(props)
}

// ── words → IR ───────────────────────────────────────────────────────

fn word_ir_quoted(w: &Word) -> IrExpr {
    match w {
        Word::CommandSubstitution(cmd, _) => match cmdsub_arith_expr(cmd) {
            Some(t) => match parse_arith(t) {
                Some(a) => IrExpr::Arith(Box::new(a)),
                None => call("arith", vec![st(t)]),
            },
            None => call(
                "capture",
                vec![IrExpr::Arrow(command_arrow_stmts(cmd))],
            ),
        },
        _ => word_ir(w),
    }
}

/// The lexer mis-reads `$(( expr ))` with a space after `((` as a command
/// substitution of a parenthesized group: `$( ( expr ) )` collapses to a
/// CommandSubstitution wrapping a bare simple command whose NAME is the
/// whitespace-padded expression (`" a + b + c "`). A normal command name
/// can never carry leading/trailing whitespace, so this shape is always an
/// arithmetic artifact — recover the expression (`$((...))` semantics).
fn cmdsub_arith_expr(cmd: &Command) -> Option<&str> {
    if let Command::Simple(sc) = cmd {
        if sc.args.is_empty() && sc.redirects.is_empty() && sc.env_vars.is_empty() {
            if let Word::Literal(s, _) = &sc.name {
                let t = s.trim();
                if !t.is_empty()
                    && s.starts_with(char::is_whitespace)
                    && s.ends_with(char::is_whitespace)
                {
                    return Some(t);
                }
            }
        }
    }
    None
}

/// Bash quote removal for BARE literal words. The AST loses the quoting
/// context (single-quoted `'a\b'` and unquoted `a\b` both arrive as
/// Literal("a\\b")), so mirror what the corpus needs: strip a backslash
/// before any char EXCEPT those that appear backslash-escaped inside
/// single-quoted literals in the corpus (printf/tr/sed escape sequences and
/// the like must survive). The perl generator applies unconditional removal;
/// this whitelist keeps every currently-passing estree example green.
fn shell_quote_removal(s: &str) -> String {
    const KEEP: &[char] = &[
        'n', '"', 'x', 'u', 't', '(', 'v', 'r', 'f', 'b', 'a', '\\', ')',
        '0', '1', '2', '3', '4', '5', '6', '7', '8', '9',
    ];
    let mut out = String::with_capacity(s.len());
    let mut chars = s.chars();
    while let Some(c) = chars.next() {
        if c == '\\' {
            match chars.next() {
                Some(next) if KEEP.contains(&next) => {
                    out.push('\\');
                    out.push(next);
                }
                Some(next) => out.push(next),
                None => {} // trailing backslash is dropped (bash behavior)
            }
        } else {
            out.push(c);
        }
    }
    out
}

fn word_ir(w: &Word) -> IrExpr {
    match w {
        Word::Literal(s, _) => st(&shell_quote_removal(s)),
        Word::Variable(name, _, _) => call("getVar", vec![st(name)]),
        Word::CommandSubstitution(cmd, _) => match cmdsub_arith_expr(cmd) {
            Some(t) => match parse_arith(t) {
                Some(a) => IrExpr::Arith(Box::new(a)),
                None => call("arith", vec![st(t)]),
            },
            None => call(
                "captureWords",
                vec![IrExpr::Arrow(command_arrow_stmts(cmd))],
            ),
        },
        Word::ParameterExpansion(pe, _) => param_ir(pe),
        Word::Arithmetic(ae, _) => match parse_arith(&ae.expression) {
            Some(a) => IrExpr::Arith(Box::new(a)),
            None => call("arith", vec![st(&ae.expression)]),
        },
        Word::BraceExpansion(be, _) => brace_ir(be),
        Word::Array(name, _, _) => call("getVar", vec![st(name)]),
        Word::MapAccess(name, key, _) => call("arrayIndex", vec![st(name), st(key)]),
        Word::MapKeys(name, _) => call("arrayItems", vec![st(name)]),
        Word::MapLength(name, _) => call("arrayLen", vec![st(name)]),
        Word::ArraySlice(name, offset, length, _) => call(
            "param",
            vec![
                st("slice"),
                st(name),
                st(offset),
                st(length.as_deref().unwrap_or("")),
            ],
        ),
        Word::StringInterpolation(interp, _) => {
            if let Some(part) = pure_template_part(interp) {
                part
            } else {
                interpolate_ir(&interp.parts)
            }
        }
        other => call("unsupported", vec![st(&other.to_string())]),
    }
}

fn param_ir(pe: &ParameterExpansion) -> IrExpr {
    let (op, extra): (String, Vec<IrExpr>) = match &pe.operator {
        ParameterExpansionOperator::None if pe.variable.len() > 1 && pe.variable.starts_with('#') => {
            // ${#name} — string length (the parser keeps the `#` in the name)
            return call("param", vec![st("len"), st(&pe.variable[1..])]);
        }
        ParameterExpansionOperator::None => (String::new(), vec![]),
        ParameterExpansionOperator::UppercaseAll => ("^^".into(), vec![]),
        ParameterExpansionOperator::LowercaseAll => (",,".into(), vec![]),
        ParameterExpansionOperator::UppercaseFirst => ("^".into(), vec![]),
        ParameterExpansionOperator::RemoveLongestPrefix(p) => ("##".into(), vec![st(p)]),
        ParameterExpansionOperator::RemoveShortestPrefix(p) => ("#".into(), vec![st(p)]),
        ParameterExpansionOperator::RemoveLongestSuffix(p) => ("%%".into(), vec![st(p)]),
        ParameterExpansionOperator::RemoveShortestSuffix(p) => ("%".into(), vec![st(p)]),
        ParameterExpansionOperator::SubstituteAll(p, r) => ("//".into(), vec![st(p), st(r)]),
        ParameterExpansionOperator::DefaultValue(d) => (":-".into(), vec![st(d)]),
        ParameterExpansionOperator::AssignDefault(d) => (":=".into(), vec![st(d)]),
        ParameterExpansionOperator::ErrorIfUnset(e) => (":?".into(), vec![st(e)]),
        ParameterExpansionOperator::Basename => ("basename".into(), vec![]),
        ParameterExpansionOperator::Dirname => ("dirname".into(), vec![]),
        ParameterExpansionOperator::ArraySlice(off, len) => (
            "slice".into(),
            vec![st(off), st(len.as_deref().unwrap_or(""))],
        ),
    };
    let mut args = vec![st(&op), st(&pe.variable)];
    args.extend(extra);
    call("param", args)
}

fn brace_ir(be: &BraceExpansion) -> IrExpr {
    call(
        "brace",
        vec![
            st(be.prefix.as_deref().unwrap_or("")),
            IrExpr::Json(serde_json::Value::Array(vec![brace_items_json(&be.items)])),
            IrExpr::Json(serde_json::Value::Array(vec![])),
            st(be.suffix.as_deref().unwrap_or("")),
        ],
    )
}

fn brace_items_json(items: &[BraceItem]) -> serde_json::Value {
    serde_json::Value::Array(
        items
            .iter()
            .map(|it| match it {
                BraceItem::Literal(s) => serde_json::Value::String(s.clone()),
                BraceItem::Range(r) => serde_json::json!({
                    "range": [r.start, r.end, r.step, r.format]
                }),
                BraceItem::Sequence(seq) => serde_json::Value::Array(
                    seq.iter().map(|s| serde_json::Value::String(s.clone())).collect(),
                ),
                BraceItem::Nested(n) => serde_json::json!({ "nested": brace_items_json(&n.items) }),
                BraceItem::Compound(c) => serde_json::json!({ "nested": brace_items_json(c) }),
            })
            .collect(),
    )
}

fn pure_template_part(interp: &StringInterpolation) -> Option<IrExpr> {
    pure_part(interp).map(part_ir)
}

/// The single non-literal part of a one-part interpolation (if any).
fn pure_part(interp: &StringInterpolation) -> Option<&StringPart> {
    let mut non_literal: Option<&StringPart> = None;
    for p in &interp.parts {
        match p {
            StringPart::Literal(s) if s.is_empty() => {}
            StringPart::Literal(_) => return None,
            other => {
                if non_literal.is_some() {
                    return None;
                }
                non_literal = Some(other);
            }
        }
    }
    non_literal
}

/// Like `part_ir` but WITHOUT the sh2.join wrapper for array-valued parts:
/// `for x in "${!map[@]}"` must iterate each key, so the array is passed
/// through (the runtime's forLoop flattens it).
fn part_ir_flat(part: &StringPart) -> IrExpr {
    match part {
        // `for x in "$@"` with NO positionals must iterate ZERO times (bash
        // runs the loop body once per positional; an empty list runs it never).
        // getVar("@") would join to "" and yield one bogus iteration.
        StringPart::Variable(name) if name == "@" => call("listVar", vec![st(name)]),
        StringPart::MapAccess(name, key) if key == "@" || key == "*" => {
            call("arrayIndex", vec![st(name), st(key)])
        }
        StringPart::MapKeys(name) => call("arrayItems", vec![st(name)]),
        StringPart::ArraySlice(name, offset, length) => call(
            "param",
            vec![
                st("slice"),
                st(name),
                st(offset),
                st(length.as_deref().unwrap_or("")),
            ],
        ),
        // `${!map[@]}` — the parser tags it as a slice of `!map`; for-loop
        // items must see the ARRAY (each key iterated), not the join.
        StringPart::ParameterExpansion(pe)
            if matches!(pe.operator, ParameterExpansionOperator::ArraySlice(..)) =>
        {
            param_ir(pe)
        }
        other => part_ir(other),
    }
}

fn interpolate_ir(parts: &[StringPart]) -> IrExpr {
    IrExpr::Interpolate(
        parts
            .iter()
            .map(|p| match p {
                StringPart::Literal(s) => InterpPart::Lit(s.clone()),
                other => InterpPart::Expr(Box::new(part_ir(other))),
            })
            .collect(),
    )
}

fn part_ir(part: &StringPart) -> IrExpr {
    match part {
        StringPart::Literal(_) => unreachable!("Literal parts handled in interpolate_ir"),
        StringPart::Variable(name) => call("getVar", vec![st(name)]),
        StringPart::ParameterExpansion(pe) => {
            // `${arr[@]:off:len}` (ArraySlice) can return an ARRAY; inside a
            // template literal that would render with JS comma joins — wrap
            // in sh2.join (idempotent for plain string slices).
            if matches!(pe.operator, ParameterExpansionOperator::ArraySlice(..)) {
                call("join", vec![param_ir(pe)])
            } else {
                param_ir(pe)
            }
        }
        StringPart::Arithmetic(ae) => match parse_arith(&ae.expression) {
            Some(a) => IrExpr::Arith(Box::new(a)),
            None => call("arith", vec![st(&ae.expression)]),
        },
        // Array-valued expansions inside a template literal would render with
        // JS's comma join; bash joins them with spaces, so wrap in sh2.join.
        // (In direct exec-arg position the array is flattened instead — see
        // word_ir / the runtime's exec.)
        StringPart::MapAccess(name, key) if key == "@" || key == "*" => call(
            "join",
            vec![call("arrayIndex", vec![st(name), st(key)])],
        ),
        StringPart::MapAccess(name, key) => call("arrayIndex", vec![st(name), st(key)]),
        StringPart::MapKeys(name) => call("join", vec![call("arrayItems", vec![st(name)])]),
        StringPart::MapLength(name) => call("arrayLen", vec![st(name)]),
        StringPart::ArraySlice(name, offset, length) => call(
            "join",
            vec![call(
                "param",
                vec![
                    st("slice"),
                    st(name),
                    st(offset),
                    st(length.as_deref().unwrap_or("")),
                ],
            )],
        ),
        StringPart::CommandSubstitution(cmd) => match cmdsub_arith_expr(cmd) {
            Some(t) => match parse_arith(t) {
                Some(a) => IrExpr::Arith(Box::new(a)),
                None => call("arith", vec![st(t)]),
            },
            None => call(
                "capture",
                vec![IrExpr::Arrow(command_arrow_stmts(cmd))],
            ),
        },
        other => call("unsupported", vec![st(&format!("{other:?}"))]),
    }
}

fn for_item_ir(w: &Word) -> IrExpr {
    match w {
        Word::Variable(name, _, _) if name == "@" || name == "*" => call("listVar", vec![st(name)]),
        Word::StringInterpolation(interp, _) => {
            if let Some(part) = pure_part(interp) {
                // Un-joined: `for x in "${!map[@]}"` iterates each element.
                return part_ir_flat(part);
            }
            word_ir(w)
        }
        _ => arg_word_ir(w, false),
    }
}

// ── arithmetic string → neutral AST ──────────────────────────────────
/// Recursive-descent parser for `$((...))` content. Returns None when the
/// expression contains assignments / ++ / -- / anything needing setVar
/// semantics — those fall back to the runtime `sh2.arith` evaluator.
fn parse_arith(src: &str) -> Option<ArithAst> {
    let chars: Vec<char> = src.chars().collect();
    let mut pos = 0usize;
    let n = chars.len();

    fn skip(chars: &[char], pos: &mut usize) {
        while *pos < chars.len() && chars[*pos].is_whitespace() {
            *pos += 1;
        }
    }
    fn eat2(chars: &[char], pos: &mut usize, s: &str) -> bool {
        if *pos + s.len() <= chars.len() {
            let got: String = chars[*pos..*pos + s.len()].iter().collect();
            if got == s {
                *pos += s.len();
                return true;
            }
        }
        false
    }
    fn primary(chars: &[char], pos: &mut usize) -> Option<ArithAst> {
        skip(chars, pos);
        if *pos >= chars.len() {
            return None;
        }
        let c = chars[*pos];
        if c == '(' {
            *pos += 1;
            let e = ternary(chars, pos)?;
            skip(chars, pos);
            if *pos >= chars.len() || chars[*pos] != ')' {
                return None;
            }
            *pos += 1;
            return Some(e);
        }
        if c.is_ascii_digit() {
            let mut s = String::new();
            while *pos < chars.len()
                && (chars[*pos].is_ascii_alphanumeric() || chars[*pos] == 'x' || chars[*pos] == 'X')
            {
                s.push(chars[*pos]);
                *pos += 1;
            }
            let v = if s.starts_with("0x") || s.starts_with("0X") {
                i64::from_str_radix(&s[2..], 16).ok()?
            } else {
                s.parse::<i64>().ok()?
            };
            return Some(ArithAst::Num(v));
        }
        if c.is_ascii_alphabetic() || c == '_' {
            let mut name = String::new();
            while *pos < chars.len()
                && (chars[*pos].is_ascii_alphanumeric() || chars[*pos] == '_')
            {
                name.push(chars[*pos]);
                *pos += 1;
            }
            skip(chars, pos);
            if *pos < chars.len() && chars[*pos] == '[' {
                *pos += 1;
                let key = ternary(chars, pos)?;
                skip(chars, pos);
                if *pos >= chars.len() || chars[*pos] != ']' {
                    return None;
                }
                *pos += 1;
                return Some(ArithAst::Index {
                    var: name,
                    key: Box::new(key),
                });
            }
            // postfix ++ / -- need setVar semantics → fall back to runtime
            if eat2(chars, pos, "++") || eat2(chars, pos, "--") {
                return None;
            }
            return Some(ArithAst::Var(name));
        }
        None
    }
    fn unary(chars: &[char], pos: &mut usize) -> Option<ArithAst> {
        skip(chars, pos);
        if *pos < chars.len() {
            let c = chars[*pos];
            if c == '-' || c == '+' || c == '!' || c == '~' {
                *pos += 1;
                let op = match c {
                    '-' => "-",
                    '+' => "+",
                    '!' => "!",
                    _ => "~",
                };
                return Some(ArithAst::Un {
                    op,
                    arg: Box::new(unary(chars, pos)?),
                });
            }
        }
        primary(chars, pos)
    }
    // ** power: RIGHT-associative (2**3**2 = 2**(3**2) = 512), binds tighter
    // than * / % (a**b * c = (a**b) * c), matching bash/evalArith.
    fn pow(chars: &[char], pos: &mut usize) -> Option<ArithAst> {
        let base = unary(chars, pos)?;
        skip(chars, pos);
        if *pos + 1 < chars.len() && chars[*pos] == '*' && chars[*pos + 1] == '*' {
            *pos += 2;
            let exp = pow(chars, pos)?;
            return Some(ArithAst::Bin {
                op: "**",
                lhs: Box::new(base),
                rhs: Box::new(exp),
            });
        }
        Some(base)
    }
    fn mul(chars: &[char], pos: &mut usize) -> Option<ArithAst> {
        let mut lhs = pow(chars, pos)?;
        loop {
            skip(chars, pos);
            let c = *chars.get(*pos).unwrap_or(&'\0');
            if c == '*' {
                *pos += 1;
                let rhs = pow(chars, pos)?;
                lhs = ArithAst::Bin {
                    op: "*",
                    lhs: Box::new(lhs),
                    rhs: Box::new(rhs),
                };
            } else if c == '/' {
                *pos += 1;
                let rhs = pow(chars, pos)?;
                lhs = ArithAst::Bin {
                    op: "/",
                    lhs: Box::new(lhs),
                    rhs: Box::new(rhs),
                };
            } else if c == '%' {
                *pos += 1;
                let rhs = pow(chars, pos)?;
                lhs = ArithAst::Bin {
                    op: "%",
                    lhs: Box::new(lhs),
                    rhs: Box::new(rhs),
                };
            } else {
                return Some(lhs);
            }
        }
    }
    fn add(chars: &[char], pos: &mut usize) -> Option<ArithAst> {
        let mut lhs = mul(chars, pos)?;
        loop {
            skip(chars, pos);
            let c = *chars.get(*pos).unwrap_or(&'\0');
            if c == '+' {
                *pos += 1;
                let rhs = mul(chars, pos)?;
                lhs = ArithAst::Bin {
                    op: "+",
                    lhs: Box::new(lhs),
                    rhs: Box::new(rhs),
                };
            } else if c == '-' {
                *pos += 1;
                let rhs = mul(chars, pos)?;
                lhs = ArithAst::Bin {
                    op: "-",
                    lhs: Box::new(lhs),
                    rhs: Box::new(rhs),
                };
            } else {
                return Some(lhs);
            }
        }
    }
    fn shift(chars: &[char], pos: &mut usize) -> Option<ArithAst> {
        let mut lhs = add(chars, pos)?;
        loop {
            skip(chars, pos);
            if eat2(chars, pos, "<<") {
                let rhs = add(chars, pos)?;
                lhs = ArithAst::Bin {
                    op: "<<",
                    lhs: Box::new(lhs),
                    rhs: Box::new(rhs),
                };
            } else if eat2(chars, pos, ">>") {
                let rhs = add(chars, pos)?;
                lhs = ArithAst::Bin {
                    op: ">>",
                    lhs: Box::new(lhs),
                    rhs: Box::new(rhs),
                };
            } else {
                return Some(lhs);
            }
        }
    }
    fn rel(chars: &[char], pos: &mut usize) -> Option<ArithAst> {
        let mut lhs = shift(chars, pos)?;
        loop {
            skip(chars, pos);
            let c = *chars.get(*pos).unwrap_or(&'\0');
            if c == '<' || c == '>' {
                let mut two = false;
                let op = if c == '<' {
                    two = *pos + 1 < chars.len() && chars[*pos + 1] == '=';
                    if two {
                        "<="
                    } else {
                        "<"
                    }
                } else {
                    two = *pos + 1 < chars.len() && chars[*pos + 1] == '=';
                    if two {
                        ">="
                    } else {
                        ">"
                    }
                };
                *pos += if two { 2 } else { 1 };
                let rhs = shift(chars, pos)?;
                lhs = ArithAst::Bin {
                    op,
                    lhs: Box::new(lhs),
                    rhs: Box::new(rhs),
                };
            } else {
                return Some(lhs);
            }
        }
    }
    fn eq(chars: &[char], pos: &mut usize) -> Option<ArithAst> {
        let mut lhs = rel(chars, pos)?;
        loop {
            skip(chars, pos);
            if eat2(chars, pos, "==") {
                let rhs = rel(chars, pos)?;
                lhs = ArithAst::Bin {
                    op: "==",
                    lhs: Box::new(lhs),
                    rhs: Box::new(rhs),
                };
            } else if eat2(chars, pos, "!=") {
                let rhs = rel(chars, pos)?;
                lhs = ArithAst::Bin {
                    op: "!=",
                    lhs: Box::new(lhs),
                    rhs: Box::new(rhs),
                };
            } else {
                return Some(lhs);
            }
        }
    }
    fn band(chars: &[char], pos: &mut usize) -> Option<ArithAst> {
        let mut lhs = eq(chars, pos)?;
        loop {
            skip(chars, pos);
            if *pos < chars.len() && chars[*pos] == '&' && chars.get(*pos + 1) != Some(&'&') {
                *pos += 1;
                let rhs = eq(chars, pos)?;
                lhs = ArithAst::Bin {
                    op: "&",
                    lhs: Box::new(lhs),
                    rhs: Box::new(rhs),
                };
            } else {
                return Some(lhs);
            }
        }
    }
    fn bxor(chars: &[char], pos: &mut usize) -> Option<ArithAst> {
        let mut lhs = band(chars, pos)?;
        loop {
            skip(chars, pos);
            if *pos < chars.len() && chars[*pos] == '^' {
                *pos += 1;
                let rhs = band(chars, pos)?;
                lhs = ArithAst::Bin {
                    op: "^",
                    lhs: Box::new(lhs),
                    rhs: Box::new(rhs),
                };
            } else {
                return Some(lhs);
            }
        }
    }
    fn bor(chars: &[char], pos: &mut usize) -> Option<ArithAst> {
        let mut lhs = bxor(chars, pos)?;
        loop {
            skip(chars, pos);
            if *pos < chars.len() && chars[*pos] == '|' && chars.get(*pos + 1) != Some(&'|') {
                *pos += 1;
                let rhs = bxor(chars, pos)?;
                lhs = ArithAst::Bin {
                    op: "|",
                    lhs: Box::new(lhs),
                    rhs: Box::new(rhs),
                };
            } else {
                return Some(lhs);
            }
        }
    }
    fn land(chars: &[char], pos: &mut usize) -> Option<ArithAst> {
        let mut lhs = bor(chars, pos)?;
        loop {
            skip(chars, pos);
            if eat2(chars, pos, "&&") {
                let rhs = bor(chars, pos)?;
                lhs = ArithAst::Bin {
                    op: "&&",
                    lhs: Box::new(lhs),
                    rhs: Box::new(rhs),
                };
            } else {
                return Some(lhs);
            }
        }
    }
    fn lor(chars: &[char], pos: &mut usize) -> Option<ArithAst> {
        let mut lhs = land(chars, pos)?;
        loop {
            skip(chars, pos);
            if eat2(chars, pos, "||") {
                let rhs = land(chars, pos)?;
                lhs = ArithAst::Bin {
                    op: "||",
                    lhs: Box::new(lhs),
                    rhs: Box::new(rhs),
                };
            } else {
                return Some(lhs);
            }
        }
    }
    fn ternary(chars: &[char], pos: &mut usize) -> Option<ArithAst> {
        let test = lor(chars, pos)?;
        skip(chars, pos);
        if *pos < chars.len() && chars[*pos] == '?' {
            *pos += 1;
            let then = ternary(chars, pos)?;
            skip(chars, pos);
            if *pos >= chars.len() || chars[*pos] != ':' {
                return None;
            }
            *pos += 1;
            let else_ = ternary(chars, pos)?;
            return Some(ArithAst::Cond {
                test: Box::new(test),
                then: Box::new(then),
                else_: Box::new(else_),
            });
        }
        Some(test)
    }

    let ast = ternary(&chars, &mut pos)?;
    skip(&chars, &mut pos);
    if pos != n {
        return None;
    }
    Some(ast)
}

/// Render the neutral arithmetic AST as native JS expressions.
fn arith_to_estree(a: &ArithAst) -> Expr {
    match a {
        ArithAst::Num(v) => Expr::Literal {
            value: serde_json::Value::from(*v),
            raw: None,
        },
        ArithAst::Var(name) => Expr::LogicalExpression {
            operator: "||",
            left: Box::new(Expr::CallExpression {
                callee: Box::new(Expr::Identifier {
                    name: "Number".to_string(),
                }),
                arguments: vec![sh2_call("getVar", vec![str_lit(name)])],
                optional: false,
            }),
            right: Box::new(Expr::Literal {
                value: serde_json::Value::from(0),
                raw: None,
            }),
        },
        ArithAst::Index { var, key } => Expr::LogicalExpression {
            operator: "||",
            left: Box::new(Expr::CallExpression {
                callee: Box::new(Expr::Identifier {
                    name: "Number".to_string(),
                }),
                arguments: vec![sh2_call("arrayIndex", vec![str_lit(var), arith_to_estree(key)])],
                optional: false,
            }),
            right: Box::new(Expr::Literal {
                value: serde_json::Value::from(0),
                raw: None,
            }),
        },
        ArithAst::Bin { op, lhs, rhs } => {
            if *op == "&&" || *op == "||" {
                // bash yields 0/1; JS logicals yield one of the operands
                Expr::ConditionalExpression {
                    test: Box::new(Expr::LogicalExpression {
                        operator: op,
                        left: Box::new(arith_to_estree(lhs)),
                        right: Box::new(arith_to_estree(rhs)),
                    }),
                    consequent: Box::new(Expr::Literal {
                        value: serde_json::Value::from(1),
                        raw: None,
                    }),
                    alternate: Box::new(Expr::Literal {
                        value: serde_json::Value::from(0),
                        raw: None,
                    }),
                }
            } else if *op == "/" {
                // bash arithmetic is INTEGER division (truncating toward
                // zero); zero divisor must abort the whole expansion, and
                // JS bitwise ops would silently absorb a NaN — so throw
                // from the runtime helper (caught by arithEval).
                sh2_call("idiv", vec![arith_to_estree(lhs), arith_to_estree(rhs)])
            } else if *op == "%" {
                // modulo by zero aborts the expansion too (bash "division by 0")
                sh2_call("imod", vec![arith_to_estree(lhs), arith_to_estree(rhs)])
            } else if matches!(*op, "<" | "<=" | ">" | ">=" | "==" | "!=") {
                // bash comparisons yield 0/1; JS yields booleans
                Expr::ConditionalExpression {
                    test: Box::new(Expr::BinaryExpression {
                        operator: op,
                        left: Box::new(arith_to_estree(lhs)),
                        right: Box::new(arith_to_estree(rhs)),
                    }),
                    consequent: Box::new(Expr::Literal {
                        value: serde_json::Value::from(1),
                        raw: None,
                    }),
                    alternate: Box::new(Expr::Literal {
                        value: serde_json::Value::from(0),
                        raw: None,
                    }),
                }
            } else {
                Expr::BinaryExpression {
                    operator: op,
                    left: Box::new(arith_to_estree(lhs)),
                    right: Box::new(arith_to_estree(rhs)),
                }
            }
        }
        ArithAst::Un { op, arg } => {
            if *op == "!" {
                // bash ! yields 0/1; JS ! yields a boolean
                Expr::ConditionalExpression {
                    test: Box::new(Expr::UnaryExpression {
                        operator: "!",
                        argument: Box::new(arith_to_estree(arg)),
                        prefix: true,
                    }),
                    consequent: Box::new(Expr::Literal {
                        value: serde_json::Value::from(1),
                        raw: None,
                    }),
                    alternate: Box::new(Expr::Literal {
                        value: serde_json::Value::from(0),
                        raw: None,
                    }),
                }
            } else {
                Expr::UnaryExpression {
                    operator: op,
                    argument: Box::new(arith_to_estree(arg)),
                    prefix: true,
                }
            }
        }
        ArithAst::Cond { test, then, else_ } => Expr::ConditionalExpression {
            test: Box::new(arith_to_estree(test)),
            consequent: Box::new(arith_to_estree(then)),
            alternate: Box::new(arith_to_estree(else_)),
        },
    }
}

// ── IR → ESTree ──────────────────────────────────────────────────────

fn is_async_call(name: &str) -> bool {
    matches!(
        name,
        "exec" | "redirect" | "pipeline" | "subshell" | "block" | "whileLoop" | "cstyleFor"
            | "capture" | "captureWords" | "forLoop" | "and" | "or"
    )
}

pub fn shir_to_estree(prog: &IrProgram) -> Program {
    Program {
        type_: "Program",
        source_type: "module",
        body: prog.stmts.iter().filter_map(top_stmt_to_estree).collect(),
    }
}

/// Top-level statement lowering: additionally wraps statement-position calls
/// in `sh2.guard(...)` so the runtime can implement `set -e` (errexit): a
/// failing SIMPLE command at statement level aborts the script, exactly like
/// bash. Guarded are single calls (exec/test/pipeline/redirect/subshell/
/// loops) and assignments; NOT guarded are `&&`/`||`/`!` expressions (bash
/// exempts non-final commands in those lists from errexit).
fn top_stmt_to_estree(stmt: &IrStmt) -> Option<Stmt> {
    let s = stmt_to_estree(stmt)?;
    let guardable = match stmt {
        IrStmt::Expr(IrExpr::Call { func, .. }) => {
            // `&&` / `||` / `!` lists: bash exempts non-final commands from
            // errexit; `and`/`or` are the lowered &&/|| (same exemption),
            // `not` is `!`.
            !matches!(
                func.as_str(),
                "break" | "continue" | "return" | "and" | "or" | "not"
            )
        }
        IrStmt::Expr(_) => false, // && / || / ! — errexit exemptions
        IrStmt::While { .. }
        | IrStmt::For { .. }
        | IrStmt::Subshell(_)
        | IrStmt::Redirect { .. }
        | IrStmt::Assign { .. } => true,
        _ => false,
    };
    if !guardable {
        return Some(s);
    }
    match s {
        Stmt::ExpressionStatement { expression } => Some(Stmt::ExpressionStatement {
            expression: sh2_call("guard", vec![expression]),
        }),
        other => Some(other),
    }
}

pub fn shir_to_estree_json(prog: &IrProgram) -> Result<String, serde_json::Error> {
    serde_json::to_string(&shir_to_estree(prog))
}

fn stmt_to_estree(stmt: &IrStmt) -> Option<Stmt> {
    Some(match stmt {
        IrStmt::Expr(IrExpr::Call { func, args, .. }) if func == "break" => {
            Stmt::BreakStatement { label: None }
        }
        IrStmt::Expr(IrExpr::Call { func, args, .. }) if func == "continue" => {
            Stmt::ContinueStatement { label: None }
        }
        IrStmt::Expr(IrExpr::Call { func, args, .. }) if func == "return" => {
            Stmt::ReturnStatement {
                argument: args.first().map(expr_to_estree),
            }
        }
        IrStmt::Expr(e) => Stmt::ExpressionStatement {
            expression: expr_to_estree(e),
        },
        IrStmt::Assign { targets, expr } => {
            let target = &targets[0];
            match expr {
                // arr=(...) / arr+=(...) / x op= v → the helper already sets
                // the variable itself; emit the call bare.
                IrExpr::Call { func, .. }
                    if func == "setArray" || func == "setArrayAppend" || func == "assign" =>
                {
                    Stmt::ExpressionStatement {
                        expression: expr_to_estree(expr),
                    }
                }
                _ => Stmt::ExpressionStatement {
                    expression: sh2_call("setVar", vec![str_lit(&target.var), expr_to_estree(expr)]),
                },
            }
        }
        IrStmt::If { cond, then, elsifs, else_ } => {
            let consequent = Box::new(Stmt::BlockStatement {
                body: then.iter().filter_map(stmt_to_estree).collect(),
            });
            let alternate: Option<Box<Stmt>> = if else_.is_empty() {
                // bash: `if c; then ...; fi` with a false condition and no
                // else leaves `$?` = 0. The runtime tracks lastExit through
                // calls only, so the false path must set it explicitly.
                Some(Box::new(Stmt::BlockStatement {
                    body: vec![Stmt::ExpressionStatement {
                        expression: sh2_call(
                            "setLastExit",
                            vec![Expr::Literal {
                                value: serde_json::Value::from(0),
                                raw: None,
                            }],
                        ),
                    }],
                }))
            } else {
                Some(match else_.as_slice() {
                    [IrStmt::If { .. }] => Box::new(
                        stmt_to_estree(&else_[0]).unwrap_or(Stmt::BlockStatement { body: vec![] }),
                    ),
                    _ => Box::new(Stmt::BlockStatement {
                        body: else_.iter().filter_map(stmt_to_estree).collect(),
                    }),
                })
            };
            Stmt::IfStatement {
                test: expr_to_estree(cond),
                consequent,
                alternate,
            }
        }
        IrStmt::While { cond, body } => Stmt::ExpressionStatement {
            expression: await_call(
                "whileLoop",
                vec![
                    arrow(vec![], IrExpr::Arrow(vec![IrStmt::Expr(cond.clone())])),
                    arrow(vec![], IrExpr::Arrow(body.clone())),
                ],
            ),
        },
        IrStmt::For { var, iter, body } => {
            let js_var = safe_ident(var);
            let mut body_stmts = vec![IrStmt::Assign {
                targets: vec![AssignTarget {
                    var: var.clone(),
                    sigil: None,
                    indices: vec![],
                }],
                expr: IrExpr::Ident(js_var.clone()),
            }];
            body_stmts.extend(body.clone());
            Stmt::ExpressionStatement {
                expression: await_call(
                    "forLoop",
                    vec![
                        expr_to_estree(iter),
                        arrow_with_param(js_var, IrExpr::Arrow(body_stmts)),
                    ],
                ),
            }
        }
        IrStmt::Function { name, body } => Stmt::ExpressionStatement {
            expression: sh2_call(
                "define",
                vec![str_lit(name), arrow(vec![], IrExpr::Arrow(body.clone()))],
            ),
        },
        IrStmt::Subshell(stmts) => Stmt::ExpressionStatement {
            expression: await_call(
                "subshell",
                vec![arrow(vec![], IrExpr::Arrow(stmts.clone()))],
            ),
        },
        IrStmt::Background(stmts) => Stmt::ExpressionStatement {
            expression: sh2_call(
                "background",
                vec![arrow(vec![], IrExpr::Arrow(stmts.clone()))],
            ),
        },
        IrStmt::Block(stmts) => Stmt::BlockStatement {
            body: stmts.iter().filter_map(stmt_to_estree).collect(),
        },
        IrStmt::Redirect { inner, redirects } => {
            // `exec 3>&1` (exec with no command): bash installs the redirects
            // permanently in the shell's own fd table. Tell the runtime to
            // persist them (it restores non-persistent redirects afterwards).
            // Only the literal `exec` builtin with NO args qualifies —
            // `: >file`, `>file` (standalone) and `true 3>&1` all restore.
            let persist = matches!(
                inner.as_slice(),
                [IrStmt::Expr(IrExpr::Call { func, args })]
                    if func == "exec"
                        && matches!(args.as_slice(), [IrExpr::Str(name, _), IrExpr::Array(a)]
                            if name == "exec" && a.is_empty())
            );
            Stmt::ExpressionStatement {
                expression: await_call(
                    "redirect",
                    vec![
                        arrow(vec![], IrExpr::Arrow(inner.clone())),
                        array(
                            redirects
                                .iter()
                                .map(|r| redirect_spec_to_estree(r, persist))
                                .collect(),
                        ),
                    ],
                ),
            }
        }
        IrStmt::Case { discriminant, clauses } => {
            let patterns: Vec<Expr> = clauses
                .iter()
                .flat_map(|c| c.patterns.iter())
                .map(|p| str_lit(p))
                .collect();
            let cases: Vec<SwitchCase> = clauses
                .iter()
                .flat_map(|c| {
                    // Source `break`/`continue` inside a case must exit the
                    // ENCLOSING loop (bash semantics), but a native JS break
                    // would only exit the switch. Turn them into runtime
                    // signals; the synthetic trailing break (chain below)
                    // stays native to terminate the switch itself.
                    let consequent: Vec<Stmt> = c
                        .body
                        .iter()
                        .filter_map(stmt_to_estree)
                        .map(|s| match s {
                            Stmt::BreakStatement { .. } => Stmt::ExpressionStatement {
                                expression: sh2_call("break", vec![]),
                            },
                            Stmt::ContinueStatement { .. } => Stmt::ExpressionStatement {
                                expression: sh2_call("continue", vec![]),
                            },
                            other => other,
                        })
                        .chain(std::iter::once(Stmt::BreakStatement { label: None }))
                        .collect();
                    c.patterns
                        .iter()
                        .map(move |p| SwitchCase {
                            type_: "SwitchCase",
                            test: Some(str_lit(p)),
                            consequent: consequent.clone(),
                        })
                })
                .collect();
            Stmt::SwitchStatement {
                discriminant: sh2_call(
                    "caseMatch",
                    vec![expr_to_estree(discriminant), array(patterns)],
                ),
                cases,
            }
        }
        IrStmt::Return(opt) => Stmt::ReturnStatement {
            argument: opt.as_ref().map(expr_to_estree),
        },
        IrStmt::Exec { cmd, args, capture: _, redirects: _, env } => {
            let mut call_args = vec![
                expr_to_estree(cmd),
                expr_to_estree(&IrExpr::Array(args.clone())),
            ];
            if !env.is_empty() {
                call_args.push(Expr::ObjectExpression {
                    properties: env
                        .iter()
                        .map(|(k, v)| prop(k, expr_to_estree(v)))
                        .collect(),
                });
            }
            Stmt::ExpressionStatement {
                expression: await_expr(sh2_call("exec", call_args)),
            }
        }
        other => unreachable!("Perl-only IR statement reached the ESTree renderer: {other:?}"),
    })
}

fn redirect_spec_to_estree(r: &IrRedirect, persist: bool) -> Expr {
    let mut props = vec![
        prop(
            "fd",
            Expr::Literal {
                value: serde_json::Value::from(r.fd.unwrap_or(0)),
                raw: None,
            },
        ),
        prop("mode", str_lit(&r.mode)),
        prop("target", expr_to_estree(&r.target)),
    ];
    if r.mode == "heredoc" || r.mode == "heredoc-tabs" {
        props.push(prop(
            "interpolate",
            Expr::Literal {
                value: serde_json::Value::Bool(r.interpolate),
                raw: None,
            },
        ));
    }
    if persist {
        props.push(prop(
            "persist",
            Expr::Literal {
                value: serde_json::Value::Bool(true),
                raw: None,
            },
        ));
    }
    Expr::ObjectExpression { properties: props }
}

fn expr_to_estree(e: &IrExpr) -> Expr {
    match e {
        IrExpr::Int(i) => Expr::Literal {
            value: serde_json::Value::from(*i),
            raw: None,
        },
        IrExpr::Str(s, _) => Expr::Literal {
            value: serde_json::Value::String(s.clone()),
            raw: None,
        },
        IrExpr::Bool(b) => Expr::Literal {
            value: serde_json::Value::Bool(*b),
            raw: None,
        },
        IrExpr::Json(v) => Expr::Literal {
            value: v.clone(),
            raw: None,
        },
        IrExpr::Var(name, _) => sh2_call("getVar", vec![str_lit(name)]),
        IrExpr::Ident(name) => Expr::Identifier { name: name.clone() },
        IrExpr::Array(elems) => Expr::ArrayExpression {
            elements: elems.iter().map(|e| Some(expr_to_estree(e))).collect(),
        },
        IrExpr::Object(props) => Expr::ObjectExpression {
            properties: props
                .iter()
                .map(|(k, v)| prop(k, expr_to_estree(v)))
                .collect(),
        },
        IrExpr::Interpolate(parts) => interpolate_to_estree(parts),
        IrExpr::Call { func, args } => {
            let call = sh2_call(func, args.iter().map(expr_to_estree).collect());
            if is_async_call(func) {
                await_expr(call)
            } else {
                call
            }
        }
        IrExpr::BinOp { op: BinOpKind::And, lhs, rhs } => {
            // bash `a && b`: run a, then run b only if a's EXIT STATUS is 0.
            // A native JS `&&` would consult the return VALUE of the left
            // operand instead — capture() returns the captured STRING and
            // assign() returns true, so `r=$(cmd) || ...` (and friends) would
            // branch on the wrong thing. The runtime helper sequences both
            // sides and checks lastExit.
            await_call(
                "and",
                vec![arrow(vec![], (**lhs).clone()), arrow(vec![], (**rhs).clone())],
            )
        }
        IrExpr::BinOp { op: BinOpKind::Or, lhs, rhs } => await_call(
            "or",
            vec![arrow(vec![], (**lhs).clone()), arrow(vec![], (**rhs).clone())],
        ),
        // `! cmd` — bash inverts the exit STATUS (so `$?` flips too); a pure
        // JS negation would leave lastExit untouched. The runtime helper
        // negates AND records the new status (`$?` reads it back).
        IrExpr::BinOp { op: BinOpKind::Not, lhs, .. } => {
            sh2_call("not", vec![expr_to_estree(lhs)])
        }
        IrExpr::Arrow(stmts) => arrow(vec![], IrExpr::Arrow(stmts.clone())),
        IrExpr::Arith(a) => {
            let inner = arith_to_estree(a);
            sh2_call(
                "arithEval",
                vec![Expr::ArrowFunctionExpression {
                    params: vec![],
                    body: ArrowBody::Expr(Box::new(inner)),
                    expression: true,
                    r#async: false,
                }],
            )
        }
        other => unreachable!("Perl-only IR expression reached the ESTree renderer: {other:?}"),
    }
}

fn interpolate_to_estree(parts: &[InterpPart]) -> Expr {
    let mut quasis = Vec::new();
    let mut expressions = Vec::new();
    let mut raw = String::new();
    for part in parts {
        match part {
            InterpPart::Lit(s) => raw.push_str(s),
            InterpPart::Expr(e) => {
                quasis.push(quasi_element(&mut raw, false));
                expressions.push(expr_to_estree(e));
            }
        }
    }
    quasis.push(quasi_element(&mut raw, true));
    Expr::TemplateLiteral { quasis, expressions }
}

fn array(elements: Vec<Expr>) -> Expr {
    Expr::ArrayExpression {
        elements: elements.into_iter().map(Some).collect(),
    }
}

fn arrow(params: Vec<Expr>, body: IrExpr) -> Expr {
    match &body {
        IrExpr::Arrow(stmts) if stmts.len() == 1 && matches!(stmts[0], IrStmt::Expr(_)) => {
            let inner = match &stmts[0] {
                IrStmt::Expr(e) => expr_to_estree(e),
                _ => unreachable!(),
            };
            Expr::ArrowFunctionExpression {
                params,
                body: ArrowBody::Expr(Box::new(inner)),
                expression: true,
                r#async: true,
            }
        }
        IrExpr::Arrow(stmts) => Expr::ArrowFunctionExpression {
            params,
            body: ArrowBody::Block(Box::new(Stmt::BlockStatement {
                body: stmts.iter().filter_map(stmt_to_estree).collect(),
            })),
            expression: false,
            r#async: true,
        },
        other => Expr::ArrowFunctionExpression {
            params,
            body: ArrowBody::Expr(Box::new(expr_to_estree(other))),
            expression: true,
            r#async: true,
        },
    }
}

fn arrow_with_param(param: String, body: IrExpr) -> Expr {
    arrow(vec![Expr::Identifier { name: param }], body)
}

fn await_call(name: &str, args: Vec<Expr>) -> Expr {
    await_expr(sh2_call(name, args))
}

fn await_expr(inner: Expr) -> Expr {
    Expr::AwaitExpression {
        argument: Box::new(inner),
    }
}

fn safe_ident(name: &str) -> String {
    const RESERVED: &[&str] = &[
        "var", "let", "const", "function", "class", "if", "else", "for", "while",
        "do", "switch", "case", "break", "continue", "return", "new", "delete",
        "typeof", "instanceof", "in", "of", "try", "catch", "finally", "throw",
        "this", "super", "import", "export", "default", "extends", "static",
        "yield", "await", "null", "true", "false", "void", "debugger", "arguments",
    ];
    if RESERVED.contains(&name) {
        format!("{name}_")
    } else {
        name.to_string()
    }
}
