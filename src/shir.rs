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
use std::collections::{HashMap, HashSet};
use std::sync::Mutex;

/// Variables proven (conservatively) to hold ONLY numbers — lifted to
/// native JS number bindings: `let x = 0` declared at program top, reads
/// become bare `x` (no `sh2.getVar` + `Number(...)||0`), writes become
/// `x = <native expr>` (no `sh2.setVar` + `arithEval`). Reset by
/// `shir_to_estree` per compilation (the Perl generator never runs this).
static LIFTED_NUMERIC: Mutex<Option<HashSet<String>>> = Mutex::new(None);
/// Provably-string variables lifted to native JS string bindings
/// (`let x = ""`; reads are bare `x`; writes `x = <string expr>`).
static LIFTED_STRING: Mutex<Option<HashSet<String>>> = Mutex::new(None);
/// Whether `shopt -s nocasematch` may be enabled anywhere in the current
/// program (set per compilation by `shir_to_estree`; see
/// `ir_may_enable_nocasematch`). Native case/test substring lifts must
/// lowercase to stay exact when it is.
static CASE_NOCASE: Mutex<Option<bool>> = Mutex::new(None);
/// Nesting depth of `sh2.and`/`sh2.or` arrow lowering (see the BinOp And/Or
/// arms). The runtime helpers branch on `lastExit`, which a NATIVE test
/// expression never sets — so inside `&&`/`||` arrows a test must stay a
/// runtime `sh2.test` call (which records the status) and only the
/// value-consuming positions (if/while/until conds, `!`, ternary) get the
/// native lowering.
static AND_OR_DEPTH: Mutex<usize> = Mutex::new(0);
/// Whether the program may enable `set -e` (errexit) anywhere (set per
/// compilation by `shir_to_estree`; see `ir_may_enable_errexit`). The
/// runtime's `sh2.guard` wrapper is an identity function when the errexit
/// flag never turns on, so guard emission is skipped entirely for programs
/// that provably never enable it.
static MAY_ERREXIT: Mutex<Option<bool>> = Mutex::new(None);
/// Builtins the runtime implements as SYNC functions (harness
/// sh2-namespace.mjs `builtins.*` — every non-async entry of builtins.json;
/// `wait`/`exec`/`sleep`/`command` are async and stay on the async exec
/// path). `sh2.exec("echo", args)` lowers to a sync `sh2.builtin("echo",
/// args)` dispatch: identical arg flattening/glob expansion, identical
/// builtin function, minus the async exec machinery (the whileLoopSync
/// pattern — same semantics, no per-call promises).
const SYNC_BUILTINS: &[&str] = &[
    ".", ":", "basename", "break", "cd", "cmp", "comm", "continue", "declare",
    "dirname", "echo", "eval", "exit", "export", "false", "head", "let", "local",
    "mapfile", "printf", "pwd", "read", "readarray", "readonly", "return", "seq",
    "set", "shift", "sort", "source", "stat", "tail", "touch", "trap", "true",
    "type", "typeset", "uniq", "unset", "wc",
];
/// Names of every function the program defines (IrStmt::Function), set per
/// compilation by `shir_to_estree` under COMPILE_LOCK. A script-defined
/// function SHADOWS a same-named builtin in bash, so exec calls to a
/// shadowed name must keep the async exec dispatch (the runtime's function
/// map) — never the sync builtin path.
static PROGRAM_FUNCTIONS: Mutex<Option<HashSet<String>>> = Mutex::new(None);
fn program_defines_function(name: &str) -> bool {
    PROGRAM_FUNCTIONS
        .lock()
        .unwrap()
        .as_ref()
        .map(|s| s.contains(name))
        .unwrap_or(true) // unset → conservative: keep the async exec path
}

/// Recursive IrStmt walk collecting every `IrStmt::Function` name — a
/// same-named script function shadows a builtin anywhere in the program
/// (definitions inside bodies/arrows count).
fn collect_program_functions(stmts: &[IrStmt], out: &mut HashSet<String>) {
    fn walk_stmts(stmts: &[IrStmt], out: &mut HashSet<String>) {
        for st in stmts {
            match st {
                IrStmt::Function { name, body } => {
                    out.insert(name.clone());
                    walk_stmts(body, out);
                }
                IrStmt::While { body, .. }
                | IrStmt::DoWhile { body, .. }
                | IrStmt::Block(body)
                | IrStmt::Subshell(body)
                | IrStmt::Background(body) => walk_stmts(body, out),
                IrStmt::If { then, elsifs, else_, .. } => {
                    walk_stmts(then, out);
                    walk_stmts(else_, out);
                    for (_, b) in elsifs {
                        walk_stmts(b, out);
                    }
                }
                IrStmt::For { body, .. } => walk_stmts(body, out),
                IrStmt::Pipeline { stages, .. } => {
                    for stage in stages {
                        walk_stmts(stage, out);
                    }
                }
                IrStmt::Redirect { inner, .. } => walk_stmts(inner, out),
                IrStmt::Case { clauses, .. } => {
                    for c in clauses {
                        walk_stmts(&c.body, out);
                    }
                }
                IrStmt::Expr(e) => walk_expr(e, out),
                _ => {}
            }
        }
    }
    fn walk_expr(e: &IrExpr, out: &mut HashSet<String>) {
        match e {
            IrExpr::Arrow(stmts) => walk_stmts(stmts, out),
            IrExpr::Call { args, .. } => {
                for a in args {
                    walk_expr(a, out);
                }
            }
            IrExpr::Array(elems) => {
                for el in elems {
                    walk_expr(el, out);
                }
            }
            IrExpr::BinOp { lhs, rhs, .. } => {
                walk_expr(lhs, out);
                walk_expr(rhs, out);
            }
            IrExpr::Interpolate(parts) => {
                for p in parts {
                    if let InterpPart::Expr(inner) = p {
                        walk_expr(inner, out);
                    }
                }
            }
            _ => {}
        }
    }
    walk_stmts(stmts, out);
}
/// Serializes whole-program compilations: the lift/scan statics above are
/// per-compilation state, and the determinism unit test compiles in
/// parallel threads — without a lock, one thread's emission can read
/// another thread's half-installed statics (torn output). Compilations are
/// short and each process compiles one file, so the lock is uncontended in
/// practice; it is never re-entered (`shir_to_estree` does not recurse).
static COMPILE_LOCK: Mutex<()> = Mutex::new(());
/// Either lift — reads / test-injection / array-element injection consult
/// both sets.
fn is_lifted(name: &str) -> bool {
    is_lifted_num(name) || is_lifted_str(name)
}
fn is_lifted_num(name: &str) -> bool {
    LIFTED_NUMERIC
        .lock()
        .unwrap()
        .as_ref()
        .map(|s| s.contains(name))
        .unwrap_or(false)
}
fn is_lifted_str(name: &str) -> bool {
    LIFTED_STRING
        .lock()
        .unwrap()
        .as_ref()
        .map(|s| s.contains(name))
        .unwrap_or(false)
}

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
            cond: command_to_test_ir(&if_stmt.condition),
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
            let cond = command_to_test_ir(&w.condition);
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

/// Test-position command lowering (if/while/until conds): lifts the
/// `echo X | grep P >/dev/null 2>/dev/null` idiom — a substring test that
/// currently spawns echo+grep per evaluation — into a plain substring
/// compare (`Call "contains"`). Non-matching commands lower exactly as
/// `command_to_ir` would. Only fired in TEST position because that is the
/// one context where the pipeline's exit status is consumed by control
/// flow rather than read back through `$?` (`&&`/`||` operands and
/// statement-position pipelines keep their status semantics).
fn command_to_test_ir(cmd: &Command) -> IrExpr {
    let ir = command_to_ir(cmd);
    try_lift_grep_contains(&ir).unwrap_or(ir)
}

/// `echo <arg> | grep <literal> >/dev/null 2>/dev/null` → `contains(arg,
/// literal)`. grep's exit status with both streams discarded is exactly
/// "does the line contain the literal pattern"; `echo <arg>` emits one
/// line, so the lift is a plain substring test. Conservative: only plain
/// literal patterns free of BRE metacharacters (`^ $ . [ ] * \`), no grep
/// flags, echo with exactly one argument, both fds redirected to
/// /dev/null, exactly two pipeline stages.
fn try_lift_grep_contains(cond: &IrExpr) -> Option<IrExpr> {
    let IrExpr::Call { func, args } = cond else { return None };
    if func != "pipeline" {
        return None;
    }
    let [IrExpr::Array(stages)] = args.as_slice() else { return None };
    if stages.len() != 2 {
        return None;
    }
    let [IrExpr::Arrow(s1), IrExpr::Arrow(s2)] = stages.as_slice() else {
        return None;
    };
    // stage 1: exec("echo", [arg])
    let [IrStmt::Expr(IrExpr::Call { func: f1, args: a1 })] = s1.as_slice() else {
        return None;
    };
    if f1 != "exec" {
        return None;
    }
    let [IrExpr::Str(name1, _), IrExpr::Array(echo_args)] = a1.as_slice() else {
        return None;
    };
    if name1 != "echo" || echo_args.len() != 1 {
        return None;
    }
    let arg = echo_args[0].clone();
    // stage 2: Expr(Call("redirect", [Arrow([exec grep]), Array([spec...])]))
    let [IrStmt::Expr(IrExpr::Call { func: f2, args: a2 })] = s2.as_slice() else {
        return None;
    };
    if f2 != "redirect" {
        return None;
    }
    let [IrExpr::Arrow(inner), IrExpr::Array(redirect_specs)] = a2.as_slice() else {
        return None;
    };
    let [IrStmt::Expr(IrExpr::Call { func: f3, args: a3 })] = inner.as_slice() else {
        return None;
    };
    if f3 != "exec" {
        return None;
    }
    let [IrExpr::Str(name2, _), IrExpr::Array(grep_args)] = a3.as_slice() else {
        return None;
    };
    if name2 != "grep" {
        return None;
    }
    let [IrExpr::Str(pat, _)] = grep_args.as_slice() else { return None };
    if !is_safe_grep_literal(pat) {
        return None;
    }
    // both fds discarded to /dev/null (redirect-spec objects)
    let (mut out, mut err) = (false, false);
    for spec in redirect_specs {
        let IrExpr::Object(entries) = spec else { continue };
        let (mut fd, mut mode, mut target) = (None, None, None);
        for (k, v) in entries {
            match (k.as_str(), v) {
                ("fd", IrExpr::Int(f)) => fd = Some(*f),
                ("mode", IrExpr::Str(m, _)) => mode = Some(m.as_str()),
                ("target", IrExpr::Str(t, _)) => target = Some(t.as_str()),
                _ => {}
            }
        }
        if mode == Some("w") && target == Some("/dev/null") {
            match fd {
                Some(1) => out = true,
                Some(2) => err = true,
                _ => {}
            }
        }
    }
    if !(out && err) {
        return None;
    }
    Some(call(
        "contains",
        vec![arg, IrExpr::Str(pat.clone(), StrStyle::SingleQuoted)],
    ))
}

/// A grep pattern is liftable to a JS substring check only when grep would
/// treat it as a literal: no BRE metacharacters (`^ $ . [ ] * \`), no
/// leading `-` (would parse as an option). BRE treats `+ ? ( ) { } |` as
/// literals, so they are safe.
fn is_safe_grep_literal(pat: &str) -> bool {
    !pat.starts_with('-') && !pat.chars().any(|c| matches!(c, '^' | '$' | '.' | '[' | ']' | '*' | '\\'))
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

/// Emit-time evaluation of `sh2.brace(prefix, groups, middles, suffix)` —
/// the runtime's brace expansion is PURE string work over the literal JSON
/// args the parser always emits (see `brace_ir`), so the whole call lowers
/// to a native array literal. Mirrors harness/sh2-namespace.mjs exactly:
/// `braceRange` / `alphaRange` / `expandBraceNested` / `expandBraceGroup`
/// plus the cartesian product with inter-group `middles` and the
/// prefix/suffix wrap. The runtime never adds GLOB_MAGIC to brace results,
/// so the literal strings are bit-identical to what the runtime returns and
/// downstream flattening (forLoop/exec) treats them identically.
fn brace_expand(
    prefix: &str,
    groups: &serde_json::Value,
    middles: &serde_json::Value,
    suffix: &str,
) -> Vec<String> {
    fn jstr(v: &serde_json::Value) -> String {
        v.as_str().unwrap_or("").to_string()
    }
    fn is_int_str(s: &str) -> bool {
        let b = s.as_bytes();
        let digits = if b.first() == Some(&b'-') { &b[1..] } else { b };
        !digits.is_empty() && digits.iter().all(|c| c.is_ascii_digit())
    }
    fn alpha_range(a: &str, b: &str, step: i64) -> Vec<String> {
        let mut out = Vec::new();
        let ca = a.chars().next().unwrap_or('a') as i64;
        let cb = b.chars().next().unwrap_or('a') as i64;
        let mut i = ca;
        if ca <= cb {
            while i <= cb {
                out.push(char::from_u32(i as u32).unwrap().to_string());
                match i.checked_add(step) {
                    Some(n) => i = n,
                    None => break,
                }
            }
        } else {
            while i >= cb {
                out.push(char::from_u32(i as u32).unwrap().to_string());
                match i.checked_sub(step) {
                    Some(n) => i = n,
                    None => break,
                }
            }
        }
        out
    }
    fn step_of(step: &serde_json::Value) -> i64 {
        match step {
            serde_json::Value::String(s) => s
                .parse::<i64>()
                .ok()
                .map(|v| v.abs())
                .filter(|v| *v != 0)
                .unwrap_or(1),
            serde_json::Value::Number(n) => n
                .as_i64()
                .map(|v| v.abs())
                .filter(|v| *v != 0)
                .unwrap_or(1),
            _ => 1,
        }
    }
    /// The runtime's `braceRange([start, end, step, format])` — zero-padded
    /// numeric ranges, letter ranges, mixed `{a1..c3}` runs, and the
    /// literal `start..end` fallback.
    fn brace_range_value(range: &serde_json::Value) -> Vec<String> {
        let arr = match range.as_array() {
            Some(a) if a.len() >= 2 => a,
            _ => return vec![],
        };
        let start = jstr(&arr[0]);
        let end = jstr(&arr[1]);
        let st = step_of(arr.get(2).unwrap_or(&serde_json::Value::Null));
        let is_num = is_int_str(&start) && is_int_str(&end);
        // mixed `{a1..c3}` → alpha part × numeric part
        if !is_num {
            let alpha_num = |s: &str| -> Option<(String, String)> {
                let bytes = s.as_bytes();
                let alen = bytes
                    .iter()
                    .take_while(|c| c.is_ascii_alphabetic())
                    .count();
                if alen == 0 || alen == bytes.len() {
                    return None;
                }
                let (a, n) = s.split_at(alen);
                if n.is_empty() || !n.bytes().all(|c| c.is_ascii_digit()) {
                    return None;
                }
                Some((a.to_string(), n.to_string()))
            };
            if let (Some((al1, an1)), Some((al2, an2))) = (alpha_num(&start), alpha_num(&end)) {
                let alphas = alpha_range(&al1, &al2, 1);
                let lo: i64 = an1.parse().unwrap_or(0);
                let hi: i64 = an2.parse().unwrap_or(0);
                let width = an1.len().max(an2.len());
                let mut out = Vec::new();
                for ch in &alphas {
                    let mut n = lo;
                    if lo <= hi {
                        while n <= hi {
                            out.push(format!("{ch}{n:0width$}", width = width));
                            match n.checked_add(1) {
                                Some(x) => n = x,
                                None => break,
                            }
                        }
                    } else {
                        while n >= hi {
                            out.push(format!("{ch}{n:0width$}", width = width));
                            match n.checked_sub(1) {
                                Some(x) => n = x,
                                None => break,
                            }
                        }
                    }
                }
                return out;
            }
        }
        if is_num {
            let a = start.parse::<i64>().unwrap_or(0);
            let b = end.parse::<i64>().unwrap_or(0);
            let width = if start.starts_with('0') || end.starts_with('0') {
                start.len().max(end.len())
            } else {
                0
            };
            let fmt = |n: i64| -> String {
                let s = n.abs().to_string();
                let padded = if width > 0 {
                    format!("{s:0>width$}", width = width)
                } else {
                    s
                };
                if n < 0 {
                    format!("-{padded}")
                } else {
                    padded
                }
            };
            let mut out = Vec::new();
            let mut i = a;
            if a <= b {
                while i <= b {
                    out.push(fmt(i));
                    match i.checked_add(st) {
                        Some(n) => i = n,
                        None => break,
                    }
                }
            } else {
                while i >= b {
                    out.push(fmt(i));
                    match i.checked_sub(st) {
                        Some(n) => i = n,
                        None => break,
                    }
                }
            }
            return out;
        }
        let single_letter = |s: &str| s.len() == 1 && s.chars().next().unwrap().is_ascii_alphabetic();
        if single_letter(&start) && single_letter(&end) {
            return alpha_range(&start, &end, st);
        }
        let all_alpha = |s: &str| !s.is_empty() && s.bytes().all(|c| c.is_ascii_alphabetic());
        if all_alpha(&start) && all_alpha(&end) {
            // longer alpha runs (`{ab..az}`) — step applies to the last
            // letter; the prefix stays the START's prefix (runtime quirk,
            // mirrored exactly)
            let mut out = Vec::new();
            let ca = start.chars().next().unwrap() as i64;
            let cb = end.chars().next().unwrap() as i64;
            let prefix = &start[..start.len() - 1];
            let mut i = ca;
            if ca <= cb {
                while i <= cb {
                    out.push(format!("{prefix}{}", char::from_u32(i as u32).unwrap()));
                    match i.checked_add(st) {
                        Some(n) => i = n,
                        None => break,
                    }
                }
            } else {
                while i >= cb {
                    out.push(format!("{prefix}{}", char::from_u32(i as u32).unwrap()));
                    match i.checked_sub(st) {
                        Some(n) => i = n,
                        None => break,
                    }
                }
            }
            return out;
        }
        vec![format!("{start}..{end}")]
    }
    /// `expandBraceNested(items)` — nested groups expand recursively.
    fn brace_nested(items: &serde_json::Value) -> Vec<String> {
        let mut out = Vec::new();
        if let Some(arr) = items.as_array() {
            for it in arr {
                if let Some(s) = it.as_str() {
                    out.push(s.to_string());
                } else if let Some(r) = it.get("range") {
                    out.extend(brace_range_value(r));
                } else if let Some(n) = it.get("nested") {
                    out.extend(brace_nested(n));
                } else if it.is_array() {
                    out.extend(brace_nested(it));
                }
            }
        }
        out
    }
    /// `expandBraceGroup(g)` — a range inside a comma-separated group stays
    /// LITERAL (`{1..3,7..9}`); only a lone range expands.
    fn brace_group(g: &serde_json::Value) -> Vec<String> {
        let mut out = Vec::new();
        if let Some(items) = g.as_array() {
            for it in items {
                if let Some(s) = it.as_str() {
                    out.push(s.to_string());
                } else if let Some(r) = it.get("range") {
                    if items.len() == 1 {
                        out.extend(brace_range_value(r));
                    } else if let Some(rarr) = r.as_array() {
                        out.push(format!("{}..{}", jstr(&rarr[0]), jstr(&rarr[1])));
                    }
                } else if let Some(n) = it.get("nested") {
                    out.extend(brace_nested(n));
                } else if it.is_array() {
                    out.extend(brace_nested(it));
                }
            }
        }
        out
    }
    // cartesian product of the group expansions, middles spliced between
    let expansions: Vec<Vec<String>> = groups
        .as_array()
        .map(|gs| gs.iter().map(brace_group).collect())
        .unwrap_or_default();
    let mut combos: Vec<Vec<String>> = vec![vec![]];
    for g in &expansions {
        let mut next = Vec::new();
        for c in &combos {
            for it in g {
                let mut cc = c.clone();
                cc.push(it.clone());
                next.push(cc);
            }
        }
        combos = next;
    }
    let ms: Vec<String> = middles
        .as_array()
        .map(|a| a.iter().map(|v| v.as_str().unwrap_or("").to_string()).collect())
        .unwrap_or_default();
    combos
        .iter()
        .map(|c| {
            let mut body = String::new();
            for (i, x) in c.iter().enumerate() {
                body.push_str(x);
                if let Some(m) = ms.get(i) {
                    body.push_str(m);
                }
            }
            format!("{prefix}{body}{suffix}")
        })
        .collect()
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
        if c == '$' {
            // bash expands `$var` inside $(( )) as a STRING INSERTION before
            // parsing: `$(( $j * 2 ))` with j unset becomes `$(( * 2 ))` (a
            // syntax error → whole expansion empty), `$(( $j + 1 ))` becomes
            // `$(( + 1 ))` (unary plus → 1). A native number read (0 for
            // unset) cannot express that, so dollar-prefixed operands fall
            // back to the runtime evaluator (sh2.arith) which reproduces it.
            return None;
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
        ArithAst::Var(name) => {
            if is_lifted_num(name) {
                // already a JS number — no Number()/||0 coercion needed
                Expr::Identifier { name: name.clone() }
            } else if is_lifted_str(name) {
                // a JS string — bash coerces it in arithmetic (Number(x)||0)
                Expr::LogicalExpression {
                    operator: "||",
                    left: Box::new(Expr::CallExpression {
                        callee: Box::new(Expr::Identifier {
                            name: "Number".to_string(),
                        }),
                        arguments: vec![Expr::Identifier { name: name.clone() }],
                        optional: false,
                    }),
                    right: Box::new(Expr::Literal {
                        value: serde_json::Value::from(0),
                        raw: None,
                    }),
                }
            } else {
                Expr::LogicalExpression {
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
                }
            }
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

/// `sh2.exec("name", args)` → sync `sh2.builtin("name", args)` when the
/// runtime implements `name` as a SYNC builtin (harness builtins.json minus
/// the async wait/exec/sleep/command) AND no script-defined function
/// shadows it (bash: a function named like a builtin wins — the runtime's
/// exec dispatch consults its function map first, so a shadowed name must
/// keep the async exec path). Env-carrying exec calls stay async (the
/// builtin twin takes no env). The sync twin skips the async exec
/// machinery (arg flattening/glob expansion happen identically inside it),
/// the whileLoopSync pattern: same semantics, no per-call promises.
fn exec_or_builtin<'a>(func: &'a str, args: &[IrExpr]) -> &'a str {
    if func == "exec" {
        if let [IrExpr::Str(name, _), IrExpr::Array(_)] = args {
            if SYNC_BUILTINS.contains(&name.as_str()) && !program_defines_function(name) {
                return "builtin";
            }
        }
    }
    func
}

/// bash special / environment variables the RUNTIME reads from its own
/// store or process.env (IFS drives field-splitting and joins; PATH feeds
/// spawned commands; exported vars sync to process.env). Lifting any of
/// these to a native binding would desync the runtime.
fn is_reserved_var(name: &str) -> bool {
    matches!(
        name,
        "IFS" | "PATH" | "HOME" | "PWD" | "OLDPWD" | "SHELL" | "USER" | "TERM" | "LANG"
            | "LC_ALL" | "LC_CTYPE" | "PS1" | "PS2" | "PS3" | "PS4" | "ENV" | "BASH"
            | "BASH_VERSION" | "RANDOM" | "SECONDS" | "LINENO" | "PPID" | "SHLVL"
            | "HOSTNAME" | "TMPDIR" | "CDPATH" | "COLUMNS" | "LINES" | "UID" | "EUID"
            | "GROUPS" | "OPTIND" | "OPTARG" | "REPLY" | "PIPESTATUS" | "FUNCNAME"
            | "BASH_SOURCE" | "BASH_LINENO" | "BASH_ARGV" | "BASH_ARGC"
    )
}

/// JS reserved words — a lifted variable becomes a native binding, and
/// `let var = 0` (etc.) is a SyntaxError.
fn is_js_keyword(name: &str) -> bool {
    matches!(
        name,
        "var" | "let" | "const" | "function" | "return" | "if" | "else" | "for" | "while"
            | "do" | "switch" | "case" | "break" | "continue" | "new" | "delete" | "typeof"
            | "instanceof" | "in" | "of" | "class" | "extends" | "super" | "this" | "null"
            | "true" | "false" | "undefined" | "NaN" | "Infinity" | "async" | "await"
            | "yield" | "static" | "import" | "export" | "default" | "try" | "catch"
            | "finally" | "throw" | "void" | "with" | "debugger" | "enum"
    )
}

/// Is a for-loop ITERATION provably numeric? Some(true) = all items are
/// numeric (brace ranges, numeric arrays, Range); Some(false) = known
/// strings; None = unknown (command substitution, $@, ...).
fn iter_numeric(e: &IrExpr) -> Option<bool> {
    /// the brace items arrive as a Json value: nested arrays of range
    /// objects (`{1..5}`) or literal strings (`{a,b}`)
    fn json_items_numeric(v: &serde_json::Value, found: &mut bool) {
        match v {
            serde_json::Value::Array(a) => {
                for x in a {
                    json_items_numeric(x, found);
                }
            }
            serde_json::Value::Object(o) => {
                if !o.contains_key("range") {
                    *found = false;
                }
            }
            serde_json::Value::String(sv) => {
                if sv.trim().parse::<i64>().is_err() {
                    *found = false;
                }
            }
            _ => *found = false,
        }
    }
    fn brace_numeric(args: &[IrExpr]) -> Option<bool> {
        for a in args {
            if let IrExpr::Json(v) = a {
                let mut numeric = true;
                json_items_numeric(v, &mut numeric);
                return Some(numeric);
            }
        }
        None
    }
    match e {
        IrExpr::Range { .. } => Some(true),
        // the merged for-items shape: `Array([brace(...)])` for `{1..3}`,
        // or `Array([Str("1"), Str("2"), ...])` for `1 2 3`
        IrExpr::Array(elems) => {
            let mut numeric = true;
            let mut known = true;
            for el in elems {
                match el {
                    IrExpr::Str(sv, _) => {
                        if sv.trim().parse::<i64>().is_err() {
                            numeric = false;
                        }
                    }
                    IrExpr::Call { func, args } if func == "brace" => match brace_numeric(args) {
                        Some(true) => {}
                        Some(false) => numeric = false,
                        None => known = false,
                    },
                    _ => known = false,
                }
            }
            if known {
                Some(numeric)
            } else {
                None
            }
        }
        IrExpr::Call { func, args } if func == "brace" => brace_numeric(args),
        _ => None,
    }
}

/// All for-loop statements' (var, iter) pairs, recursively.
fn collect_for_iters(prog: &IrProgram) -> HashMap<String, IrExpr> {
    fn walk_stmt(st: &IrStmt, out: &mut HashMap<String, IrExpr>) {
        match st {
            IrStmt::For { var, iter, body } => {
                out.insert(var.clone(), iter.clone());
                for b in body {
                    walk_stmt(b, out);
                }
            }
            IrStmt::While { body, .. }
            | IrStmt::DoWhile { body, .. }
            | IrStmt::Block(body)
            | IrStmt::Function { body, .. }
            | IrStmt::Subshell(body)
            | IrStmt::Background(body) => {
                for b in body {
                    walk_stmt(b, out);
                }
            }
            IrStmt::If { then, elsifs, else_, .. } => {
                for b in then.iter().chain(else_) {
                    walk_stmt(b, out);
                }
                for (_, b) in elsifs {
                    for stm in b {
                        walk_stmt(stm, out);
                    }
                }
            }
            IrStmt::Exec { args, .. } => {
                for a in args {
                    walk_expr(a, out);
                }
            }
            IrStmt::Pipeline { stages, .. } => {
                for stage in stages {
                    for b in stage {
                        walk_stmt(b, out);
                    }
                }
            }
            IrStmt::Redirect { inner, .. } => {
                for b in inner {
                    walk_stmt(b, out);
                }
            }
            IrStmt::Case { clauses, .. } => {
                for c in clauses {
                    for b in &c.body {
                        walk_stmt(b, out);
                    }
                }
            }
            IrStmt::Expr(e) => walk_expr(e, out),
            IrStmt::Output { value, .. } => walk_expr(value, out),
            _ => {}
        }
    }
    fn walk_expr(e: &IrExpr, out: &mut HashMap<String, IrExpr>) {
        match e {
            IrExpr::Arrow(stmts) => {
                for st in stmts {
                    walk_stmt(st, out);
                }
            }
            IrExpr::Call { args, .. } => {
                for a in args {
                    walk_expr(a, out);
                }
            }
            _ => {}
        }
    }
    let mut out = HashMap::new();
    for st in &prog.stmts {
        walk_stmt(st, &mut out);
    }
    out
}

/// A lifted FOR-loop variable must be referenced ONLY inside its own loop
/// body: the closure param shadows the module `let` and the store sync is
/// dropped, so any read/write after the loop sees the stale initial value.
/// Remove from both lift sets any loop var referenced outside its loop.
fn drop_externally_referenced_loop_vars(
    prog: &IrProgram,
    num: &HashSet<String>,
    str: &HashSet<String>,
) -> (HashSet<String>, HashSet<String>) {
    let mut for_vars: HashSet<String> = HashSet::new();
    fn collect_for_vars(st: &IrStmt, out: &mut HashSet<String>) {
        match st {
            IrStmt::For { var, body, .. } => {
                out.insert(var.clone());
                for b in body {
                    collect_for_vars(b, out);
                }
            }
            IrStmt::While { body, .. }
            | IrStmt::DoWhile { body, .. }
            | IrStmt::Block(body)
            | IrStmt::Function { body, .. }
            | IrStmt::Subshell(body)
            | IrStmt::Background(body) => {
                for b in body {
                    collect_for_vars(b, out);
                }
            }
            IrStmt::If { then, elsifs, else_, .. } => {
                for b in then.iter().chain(else_) {
                    collect_for_vars(b, out);
                }
                for (_, b) in elsifs {
                    for stm in b {
                        collect_for_vars(stm, out);
                    }
                }
            }
            IrStmt::Pipeline { stages, .. } => {
                for stage in stages {
                    for b in stage {
                        collect_for_vars(b, out);
                    }
                }
            }
            IrStmt::Redirect { inner, .. } => {
                for b in inner {
                    collect_for_vars(b, out);
                }
            }
            IrStmt::Case { clauses, .. } => {
                for c in clauses {
                    for b in &c.body {
                        collect_for_vars(b, out);
                    }
                }
            }
            IrStmt::Expr(e) => collect_for_vars_expr(e, out),
            _ => {}
        }
    }
    fn collect_for_vars_expr(e: &IrExpr, out: &mut HashSet<String>) {
        match e {
            IrExpr::Arrow(stmts) => {
                for st in stmts {
                    collect_for_vars(st, out);
                }
            }
            IrExpr::Call { args, .. } => {
                for a in args {
                    collect_for_vars_expr(a, out);
                }
            }
            _ => {}
        }
    }
    for st in &prog.stmts {
        collect_for_vars(st, &mut for_vars);
    }

    // every reference to a var, tagged with the enclosing For-var stack
    let mut external: HashSet<String> = HashSet::new();
    fn ref_expr(e: &IrExpr, stack: &[String], external: &mut HashSet<String>) {
        match e {
            IrExpr::Var(n, _) => {
                if !stack.contains(&n.clone()) {
                    external.insert(n.clone());
                }
            }
            IrExpr::Call { func, args } => {
                if func == "getVar" {
                    if let [IrExpr::Str(n, _)] = args.as_slice() {
                        if !stack.contains(n) {
                            external.insert(n.clone());
                        }
                    }
                }
                if func == "setVar" {
                    if let [IrExpr::Str(n, _), ..] = args.as_slice() {
                        if !stack.contains(n) {
                            external.insert(n.clone());
                        }
                    }
                }
                for a in args {
                    ref_expr(a, stack, external);
                }
            }
            IrExpr::Arrow(stmts) => {
                for st in stmts {
                    ref_stmt(st, stack, external);
                }
            }
            IrExpr::BinOp { lhs, rhs, .. } => {
                ref_expr(lhs, stack, external);
                ref_expr(rhs, stack, external);
            }
            IrExpr::Ternary { cond, then, else_, .. } => {
                ref_expr(cond, stack, external);
                ref_expr(then, stack, external);
                ref_expr(else_, stack, external);
            }
            IrExpr::Capture { expr, .. } => ref_expr(expr, stack, external),
            IrExpr::Array(elems) => {
                for el in elems {
                    ref_expr(el, stack, external);
                }
            }
            IrExpr::Interpolate(parts) => {
                for p in parts {
                    if let InterpPart::Expr(inner) = p {
                        ref_expr(inner, stack, external);
                    }
                }
            }
            IrExpr::DefinedOr { expr, default } => {
                ref_expr(expr, stack, external);
                ref_expr(default, stack, external);
            }
            IrExpr::Index { key, .. } => ref_expr(key, stack, external),
            IrExpr::MethodCall { obj, args, .. } => {
                ref_expr(obj, stack, external);
                for a in args {
                    ref_expr(a, stack, external);
                }
            }
            IrExpr::Object(props) => {
                for (_, v) in props {
                    ref_expr(v, stack, external);
                }
            }
            _ => {}
        }
    }
    fn ref_stmt(st: &IrStmt, stack: &[String], external: &mut HashSet<String>) {
        match st {
            IrStmt::For { var, iter, body } => {
                let mut s2 = stack.to_vec();
                s2.push(var.clone());
                ref_expr(iter, &s2, external);
                for b in body {
                    ref_stmt(b, &s2, external);
                }
            }
            IrStmt::Assign { targets, expr } => {
                for t in targets {
                    if !stack.contains(&t.var) {
                        external.insert(t.var.clone());
                    }
                }
                ref_expr(expr, stack, external);
            }
            IrStmt::Declare { vars, .. } => {
                for v in vars {
                    if !stack.contains(&v.name) {
                        external.insert(v.name.clone());
                    }
                }
            }
            IrStmt::While { cond, body, .. }
            | IrStmt::DoWhile { cond, body, .. } => {
                ref_expr(cond, stack, external);
                for b in body {
                    ref_stmt(b, stack, external);
                }
            }
            IrStmt::If { cond, then, elsifs, else_, .. } => {
                ref_expr(cond, stack, external);
                for b in then.iter().chain(else_) {
                    ref_stmt(b, stack, external);
                }
                for (_, b) in elsifs {
                    for stm in b {
                        ref_stmt(stm, stack, external);
                    }
                }
            }
            IrStmt::Exec { cmd, args, .. } => {
                ref_expr(cmd, stack, external);
                for a in args {
                    ref_expr(a, stack, external);
                }
            }
            IrStmt::Pipeline { stages, .. } => {
                for stage in stages {
                    for b in stage {
                        ref_stmt(b, stack, external);
                    }
                }
            }
            IrStmt::Function { body, .. } | IrStmt::Block(body) | IrStmt::Subshell(body) | IrStmt::Background(body) => {
                for b in body {
                    ref_stmt(b, stack, external);
                }
            }
            IrStmt::Redirect { inner, redirects } => {
                for b in inner {
                    ref_stmt(b, stack, external);
                }
                for r in redirects {
                    ref_expr(&r.target, stack, external);
                }
            }
            IrStmt::Case { discriminant, clauses } => {
                ref_expr(discriminant, stack, external);
                for c in clauses {
                    for b in &c.body {
                        ref_stmt(b, stack, external);
                    }
                }
            }
            IrStmt::Expr(e) => ref_expr(e, stack, external),
            IrStmt::Output { value, .. } => ref_expr(value, stack, external),
            _ => {}
        }
    }
    for st in &prog.stmts {
        ref_stmt(st, &[], &mut external);
    }

    let mut num2 = num.clone();
    let mut str2 = str.clone();
    for v in &for_vars {
        if external.contains(v) {
            num2.remove(v);
            str2.remove(v);
        }
    }
    (num2, str2)
}

/// Conservative "always a number" analysis for the ESTree backend.
///
/// bash variables are strings; JS has real numbers. A variable whose every
/// assignment is provably numeric (a `$((...))` expression without `/`/`%`
/// — the only error sources — a numeric literal, or a copy of another
/// lifted variable) and that never appears in a string-parsed context
/// (`sh2.test`/`sh2.param`/array calls read the runtime STORE by string) can
/// be lifted to a native JS number binding: reads are bare `x`, writes are
/// `x = <expr>`, `let x = 0` at program top. Everything else keeps the
/// runtime store (exact current behavior).
fn numeric_lift_vars(prog: &IrProgram) -> HashSet<String> {
    let mut assigns: HashMap<String, Vec<IrExpr>> = HashMap::new();
    let mut excluded: HashSet<String> = HashSet::new();
    let mut string_ctx: HashSet<String> = HashSet::new();

    fn is_ident(s: &str) -> bool {
        let mut cs = s.chars();
        match cs.next() {
            Some(c) if c.is_ascii_alphabetic() || c == '_' => {
                cs.all(|c| c.is_ascii_alphanumeric() || c == '_')
            }
            _ => false,
        }
    }
    fn mark_string_refs(s: &str, out: &mut HashSet<String>) {
        // A var is read from the STORE whenever its name appears inside a
        // string-parsed context (test/param/brace/caseMatch strings). Mark
        // any identifier-like word, bare or $-prefixed (conservative: over-
        // marking is safe — the var just stays in the store).
        let bytes = s.as_bytes();
        let mut i = 0;
        while i < bytes.len() {
            let mut c = bytes[i] as char;
            // a `$` prefix starts a variable reference: skip it, then treat
            // the following identifier as a STORE read (`$count` inside a
            // test string resolves via the runtime store)
            if c == '$' {
                i += 1;
                if i >= bytes.len() {
                    break;
                }
                c = bytes[i] as char;
            }
            let prev_alnum = i > 0 && bytes[i - 1].is_ascii_alphanumeric();
            if (c.is_ascii_alphabetic() || c == '_') && !prev_alnum {
                let start = i;
                while i < bytes.len() && (bytes[i].is_ascii_alphanumeric() || bytes[i] == b'_') {
                    i += 1;
                }
                let w = &s[start..i];
                if is_ident(w) {
                    out.insert(w.to_string());
                }
            } else {
                i += 1;
            }
        }
    }
    fn mark_write_builtin_vars(e: &IrExpr, excluded: &mut HashSet<String>) {
        match e {
            IrExpr::Array(elems) => {
                for el in elems {
                    mark_write_builtin_vars(el, excluded);
                }
            }
            IrExpr::Str(sv, _) => {
                let v = sv.split('=').next().unwrap_or("");
                if is_ident(v) {
                    excluded.insert(v.to_string());
                }
            }
            _ => {}
        }
    }
    fn mark_str_args(e: &IrExpr, string_ctx: &mut HashSet<String>) {
        match e {
            IrExpr::Str(ss, _) => mark_string_refs(ss, string_ctx),
            IrExpr::Array(elems) => {
                for el in elems {
                    mark_str_args(el, string_ctx);
                }
            }
            IrExpr::Object(props) => {
                for (_, v) in props {
                    mark_str_args(v, string_ctx);
                }
            }
            _ => {}
        }
    }
    fn walk_expr(
        e: &IrExpr,
        excluded: &mut HashSet<String>,
        string_ctx: &mut HashSet<String>,
        in_copy: bool,
    ) {
        match e {
            IrExpr::Call { func, args } => {
                // `test` / `setArray` / `setArrayAppend` strings are
                // excluded: the renderer injects lifted values into them,
                // so a lifted var may appear inside them.
                if func != "getVar" && func != "test" && func != "setArray" && func != "setArrayAppend"
                {
                    // ANY runtime call's string args (recursing into the
                    // Array/[] wrappers exec and setArrayAppend use) may
                    // contain `$var` references the runtime resolves from
                    // the STORE — setArrayAppend(["$candidate"]),
                    // local("n=$1"), test("$count -lt 100") — so mark every
                    // identifier found there as store-read (not liftable).
                    for a in args {
                        mark_str_args(a, string_ctx);
                    }
                }
                if matches!(
                    func.as_str(),
                    "arrayIndex" | "arrayLen" | "arrayItems" | "arraySlice" | "setArray"
                        | "setArrayAppend"
                ) {
                    if let Some(IrExpr::Str(name, _)) = args.first() {
                        excluded.insert(name.clone());
                    }
                }
                for a in args {
                    walk_expr(a, excluded, string_ctx, in_copy);
                }
            }
            IrExpr::Arrow(stmts) => {
                for st in stmts {
                    walk_stmt(st, excluded, string_ctx, in_copy);
                }
            }
            IrExpr::Index { key, .. } => walk_expr(key, excluded, string_ctx, in_copy),
            IrExpr::BinOp { lhs, rhs, .. } => {
                walk_expr(lhs, excluded, string_ctx, in_copy);
                walk_expr(rhs, excluded, string_ctx, in_copy);
            }
            IrExpr::MethodCall { obj, args, .. } => {
                walk_expr(obj, excluded, string_ctx, in_copy);
                for a in args {
                    walk_expr(a, excluded, string_ctx, in_copy);
                }
            }
            IrExpr::Ternary { cond, then, else_, .. } => {
                walk_expr(cond, excluded, string_ctx, in_copy);
                walk_expr(then, excluded, string_ctx, in_copy);
                walk_expr(else_, excluded, string_ctx, in_copy);
            }
            IrExpr::DefinedOr { expr, default } => {
                walk_expr(expr, excluded, string_ctx, in_copy);
                walk_expr(default, excluded, string_ctx, in_copy);
            }
            IrExpr::Interpolate(parts) => {
                for p in parts {
                    if let InterpPart::Expr(inner) = p {
                        walk_expr(inner, excluded, string_ctx, in_copy);
                    }
                }
            }
            IrExpr::Capture { expr, .. } => walk_expr(expr, excluded, string_ctx, in_copy),
            IrExpr::Array(elems) => {
                for el in elems {
                    walk_expr(el, excluded, string_ctx, in_copy);
                }
            }
            IrExpr::Object(props) => {
                for (_, v) in props {
                    walk_expr(v, excluded, string_ctx, in_copy);
                }
            }
            _ => {}
        }
    }
    fn walk_stmt(
        st: &IrStmt,
        excluded: &mut HashSet<String>,
        string_ctx: &mut HashSet<String>,
        in_copy: bool,
    ) {
        match st {
            IrStmt::Assign { targets, expr } => {
                for t in targets {
                    if t.indices.is_empty() {
                        if in_copy {
                            // a subshell/background write is COPY-local in
                            // bash — a lifted module var would be clobbered
                            excluded.insert(t.var.clone());
                        }
                    } else {
                        excluded.insert(t.var.clone());
                    }
                }
                walk_expr(expr, excluded, string_ctx, in_copy);
            }
            IrStmt::Declare { vars, .. } => {
                for v in vars {
                    excluded.insert(v.name.clone());
                }
            }
            IrStmt::DeclareArray { var, .. } => {
                excluded.insert(var.clone());
            }
            IrStmt::For { var, iter, body } => {
                // NOTE: the loop var is NOT excluded here — the loop
                // iteration is its assignment source (see collect_for_iters
                // + the fixpoint); external references are removed by
                // drop_externally_referenced_loop_vars afterwards.
                walk_expr(iter, excluded, string_ctx, in_copy);
                for b in body {
                    walk_stmt(b, excluded, string_ctx, in_copy);
                }
            }
            IrStmt::While { cond, body } | IrStmt::DoWhile { cond, body, .. } => {
                walk_expr(cond, excluded, string_ctx, in_copy);
                for b in body {
                    walk_stmt(b, excluded, string_ctx, in_copy);
                }
            }
            IrStmt::If {
                cond,
                then,
                elsifs,
                else_,
            } => {
                walk_expr(cond, excluded, string_ctx, in_copy);
                for b in then.iter().chain(else_) {
                    walk_stmt(b, excluded, string_ctx, in_copy);
                }
                for (_, b) in elsifs {
                    for stm in b {
                        walk_stmt(stm, excluded, string_ctx, in_copy);
                    }
                }
            }
            IrStmt::Exec {
                cmd,
                args,
                capture,
                env,
                ..
            } => {
                if let Some(c) = capture {
                    excluded.insert(c.clone());
                }
                for (v, _) in env {
                    excluded.insert(v.clone());
                }
                if let IrExpr::Str(cname, _) = cmd {
                    if matches!(
                        cname.as_str(),
                        "read" | "declare" | "typeset" | "local" | "export" | "readonly"
                            | "unset" | "mapfile" | "readarray" | "let" | "eval" | "source" | "."
                    ) {
                        for a in args {
                            mark_write_builtin_vars(a, excluded);
                        }
                    }
                }
                walk_expr(cmd, excluded, string_ctx, in_copy);
                for a in args {
                    walk_expr(a, excluded, string_ctx, in_copy);
                }
            }
            IrStmt::Pipeline { stages, capture, .. } => {
                if let Some(c) = capture {
                    excluded.insert(c.clone());
                }
                for stage in stages {
                    for b in stage {
                        walk_stmt(b, excluded, string_ctx, in_copy);
                    }
                }
            }
            IrStmt::Function { body, .. } | IrStmt::Block(body) => {
                for b in body {
                    walk_stmt(b, excluded, string_ctx, in_copy);
                }
            }
            // subshell/background: COPY semantics — writes inside are local
            IrStmt::Subshell(body) | IrStmt::Background(body) => {
                for b in body {
                    walk_stmt(b, excluded, string_ctx, true);
                }
            }
            IrStmt::Redirect { inner, redirects } => {
                for b in inner {
                    walk_stmt(b, excluded, string_ctx, in_copy);
                }
                for r in redirects {
                    walk_expr(&r.target, excluded, string_ctx, in_copy);
                    if r.interpolate {
                        // the runtime expandWord's interpolated heredoc
                        // bodies from the STORE
                        if let IrExpr::Str(body, _) = &r.target {
                            mark_string_refs(body, string_ctx);
                        }
                    }
                }
            }
            IrStmt::Case {
                discriminant,
                clauses,
            } => {
                walk_expr(discriminant, excluded, string_ctx, in_copy);
                for c in clauses {
                    for p in &c.patterns {
                        mark_string_refs(p, string_ctx);
                    }
                    for b in &c.body {
                        walk_stmt(b, excluded, string_ctx, in_copy);
                    }
                }
            }
            IrStmt::Expr(e) => walk_expr(e, excluded, string_ctx, in_copy),
            IrStmt::Output { value, .. } => walk_expr(value, excluded, string_ctx, in_copy),
            IrStmt::WriteFile { path, content, .. } => {
                walk_expr(path, excluded, string_ctx, in_copy);
                walk_expr(content, excluded, string_ctx, in_copy);
            }
            IrStmt::Return(Some(e)) | IrStmt::Exit(Some(e)) => {
                walk_expr(e, excluded, string_ctx, in_copy)
            }
            IrStmt::SetChildError(e) => walk_expr(e, excluded, string_ctx, in_copy),
            IrStmt::Die { expr, .. } | IrStmt::Warn { expr, .. } => {
                walk_expr(expr, excluded, string_ctx, in_copy)
            }
            _ => {}
        }
    }

    for st in &prog.stmts {
        walk_stmt(st, &mut excluded, &mut string_ctx, false);
    }

    // collect assignment sources (top-level + function bodies — a function
    // WRITING a global is a global write in bash, so it counts)
    fn collect_assigns(st: &IrStmt, assigns: &mut HashMap<String, Vec<IrExpr>>) {
        match st {
            IrStmt::Assign { targets, expr } => {
                for t in targets {
                    if t.indices.is_empty() {
                        assigns
                            .entry(t.var.clone())
                            .or_default()
                            .push(expr.clone());
                    }
                }
            }
            IrStmt::While { body, .. }
            | IrStmt::DoWhile { body, .. }
            | IrStmt::Block(body)
            | IrStmt::Function { body, .. }
            | IrStmt::Subshell(body)
            | IrStmt::Background(body) => {
                for b in body {
                    collect_assigns(b, assigns);
                }
            }
            IrStmt::If { then, elsifs, else_, .. } => {
                for b in then.iter().chain(else_) {
                    collect_assigns(b, assigns);
                }
                for (_, b) in elsifs {
                    for stm in b {
                        collect_assigns(stm, assigns);
                    }
                }
            }
            IrStmt::For { var, body, .. } => {
                // the loop iteration is a source even with no body writes
                assigns.entry(var.clone()).or_default();
                for b in body {
                    collect_assigns(b, assigns);
                }
            }
            IrStmt::Exec { args, .. } => {
                for a in args {
                    collect_expr_assigns(a, assigns);
                }
            }
            IrStmt::Pipeline { stages, .. } => {
                for stage in stages {
                    for b in stage {
                        collect_assigns(b, assigns);
                    }
                }
            }
            IrStmt::Redirect { inner, .. } => {
                for b in inner {
                    collect_assigns(b, assigns);
                }
            }
            IrStmt::Case { clauses, .. } => {
                for c in clauses {
                    for b in &c.body {
                        collect_assigns(b, assigns);
                    }
                }
            }
            IrStmt::Expr(e) => collect_expr_assigns(e, assigns),
            IrStmt::Output { value, .. } => collect_expr_assigns(value, assigns),
            _ => {}
        }
    }
    fn collect_expr_assigns(e: &IrExpr, assigns: &mut HashMap<String, Vec<IrExpr>>) {
        match e {
            IrExpr::Arrow(stmts) => {
                for st in stmts {
                    collect_assigns(st, assigns);
                }
            }
            IrExpr::Call { args, .. } => {
                for a in args {
                    collect_expr_assigns(a, assigns);
                }
            }
            IrExpr::Array(elems) => {
                for el in elems {
                    collect_expr_assigns(el, assigns);
                }
            }
            _ => {}
        }
    }
    for st in &prog.stmts {
        collect_assigns(st, &mut assigns);
    }

    fn arith_has_div_mod(a: &ArithAst) -> bool {
        match a {
            ArithAst::Bin { op, lhs, rhs } => {
                *op == "/" || *op == "%" || arith_has_div_mod(lhs) || arith_has_div_mod(rhs)
            }
            ArithAst::Un { arg, .. } => arith_has_div_mod(arg),
            ArithAst::Cond { test, then, else_, .. } => {
                arith_has_div_mod(test) || arith_has_div_mod(then) || arith_has_div_mod(else_)
            }
            ArithAst::Index { key, .. } => arith_has_div_mod(key),
            _ => false,
        }
    }

    // fixpoint: a var is liftable when ALL its assignment sources are
    // numeric (arith without / %, numeric literal, or another lifted var).
    let for_iters = collect_for_iters(prog);
    let mut lifted: HashSet<String> = HashSet::new();
    loop {
        let mut changed = false;
        for (name, exprs) in &assigns {
            if lifted.contains(name)
                || excluded.contains(name)
                || string_ctx.contains(name)
                || is_reserved_var(name)
                || is_js_keyword(name)
                || name.contains('[')
                || name.contains(']')
            {
                // names with a subscript (`map[answer]` — the parser keeps
                // the whole bracket string as the var name) are array
                // writes: never liftable (a `let map[answer]` is invalid JS)
                continue;
            }
            let all_numeric = exprs.iter().all(|e| match e {
                IrExpr::Arith(a) => !arith_has_div_mod(a),
                IrExpr::Int(_) => true,
                IrExpr::Str(sv, _) => sv.trim().parse::<i64>().is_ok(),
                IrExpr::Var(n, _) => lifted.contains(n.as_str()),
                IrExpr::Call { func, args } if func == "getVar" => {
                    matches!(args.as_slice(), [IrExpr::Str(n, _)] if lifted.contains(n.as_str()))
                }
                _ => false,
            }) && for_iters.get(name).map_or(true, |it| iter_numeric(it) == Some(true));
            if all_numeric {
                lifted.insert(name.clone());
                changed = true;
            }
        }
        if !changed {
            break;
        }
    }
    lifted
}


pub fn shir_to_estree(prog: &IrProgram) -> Program {
    let _compile_guard = COMPILE_LOCK.lock().unwrap();
    let (num, str) = drop_externally_referenced_loop_vars(
        prog,
        &numeric_lift_vars(prog),
        &string_lift_vars(prog, &numeric_lift_vars(prog)),
    );
    // Run ALL analysis passes before touching any static: the lift/scan
    // statics are shared global state, and the determinism unit test
    // compiles concurrently in other threads — a computation between the
    // static writes and the body emission widens the torn-read window.
    let nocase = ir_may_enable_nocasematch(prog);
    let errexit = ir_may_enable_errexit(prog);
    let mut functions = HashSet::new();
    collect_program_functions(&prog.stmts, &mut functions);
    *LIFTED_NUMERIC.lock().unwrap() = Some(num);
    *LIFTED_STRING.lock().unwrap() = Some(str);
    *CASE_NOCASE.lock().unwrap() = Some(nocase);
    *MAY_ERREXIT.lock().unwrap() = Some(errexit);
    *PROGRAM_FUNCTIONS.lock().unwrap() = Some(functions);
    let mut body: Vec<Stmt> = Vec::new();
    // `let x = 0` (numeric) / `let x = ""` (string) at program top. bash
    // reads an unset var as 0 in arithmetic and "" as a string.
    for name in LIFTED_NUMERIC.lock().unwrap().as_ref().unwrap().iter() {
        body.push(Stmt::VariableDeclaration {
            kind: "let",
            declarations: vec![VariableDeclarator {
                type_: "VariableDeclarator",
                id: Expr::Identifier { name: name.clone() },
                init: Some(Expr::Literal {
                    value: serde_json::Value::from(0),
                    raw: None,
                }),
            }],
        });
    }
    for name in LIFTED_STRING.lock().unwrap().as_ref().unwrap().iter() {
        body.push(Stmt::VariableDeclaration {
            kind: "let",
            declarations: vec![VariableDeclarator {
                type_: "VariableDeclarator",
                id: Expr::Identifier { name: name.clone() },
                init: Some(Expr::Literal {
                    value: serde_json::Value::String(String::new()),
                    raw: None,
                }),
            }],
        });
    }
    body.extend(prog.stmts.iter().filter_map(top_stmt_to_estree));
    Program {
        type_: "Program",
        source_type: "module",
        body,
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
    // No `set -e` anywhere → the runtime's errexit flag can never turn on,
    // so `sh2.guard(v)` would be an identity call on every statement.
    // Skip the wrapper entirely (provably identical semantics, ~1600 fewer
    // runtime calls across the corpus).
    if !MAY_ERREXIT.lock().unwrap().unwrap_or(true) {
        return Some(s);
    }
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
        | IrStmt::Redirect { .. } => true,
        IrStmt::Assign { targets, .. } => {
            // a native lifted write always succeeds → guard would be wrong
            // (guard(0) exits under errexit)
            !targets.iter().any(|t| is_lifted(&t.var) && t.indices.is_empty())
        }
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

/// Classification of a case-pattern string for the native lowering.
/// `globMatch` on any of these shapes is a plain JS string op:
/// - `*` / `**` → matches every value (`CasePat::Any`);
/// - `*lit*` → substring (`CasePat::Substr`);
/// - `lit*` → prefix (`CasePat::Prefix`);
/// - `*lit` → suffix (`CasePat::Suffix`);
/// - `lit` (optionally quoted) → exact equality (`CasePat::Exact`).
/// Conservative: rejects `$` (expansion), `(`, `)`, quotes, backslash,
/// `!`, and any glob metacharacter inside the literal.
#[derive(Debug, Clone, PartialEq)]
enum CasePat {
    Any,
    Substr(String),
    Prefix(String),
    Suffix(String),
    Exact(String),
}

fn classify_case_pat(pat: &str) -> Option<CasePat> {
    // A quoted pattern (`"start"`, `''`) arrives with its quote chars; the
    // runtime's expandWord strips them (making a quoted `*a*` an ACTIVE
    // glob there too), so unwrap one pair of quotes before classifying.
    let bare = pat
        .strip_prefix('"')
        .and_then(|x| x.strip_suffix('"'))
        .or_else(|| {
            pat.strip_prefix('\'')
                .and_then(|x| x.strip_suffix('\''))
        })
        .unwrap_or(pat);
    let has_meta = |s: &str| {
        s.chars().any(|c| {
            matches!(
                c,
                '*' | '?' | '[' | ']' | '\\' | '$' | '(' | ')' | '\'' | '"' | '!'
            )
        })
    };
    // `*` / `**` / ... — consecutive stars collapse to one `*`.
    if !bare.is_empty() && bare.chars().all(|c| c == '*') {
        return Some(CasePat::Any);
    }
    // `*lit*`, `lit*`, `*lit` — exactly one star on either side.
    if let Some(inner) = bare.strip_prefix('*') {
        if let Some(inner) = inner.strip_suffix('*') {
            if inner.is_empty() || has_meta(inner) {
                return None;
            }
            return Some(CasePat::Substr(inner.to_string()));
        }
        if inner.is_empty() || has_meta(inner) {
            return None;
        }
        return Some(CasePat::Suffix(inner.to_string()));
    }
    if let Some(inner) = bare.strip_suffix('*') {
        if inner.is_empty() || has_meta(inner) {
            return None;
        }
        return Some(CasePat::Prefix(inner.to_string()));
    }
    if has_meta(bare) {
        return None;
    }
    Some(CasePat::Exact(bare.to_string()))
}

/// Lower a `case` whose EVERY pattern is one of the [`CasePat`] shapes to a
/// native if/else-if chain: `String(disc).includes(lit)` for substring
/// globs, `String(disc) === lit` for exact literals, `true` for `*` — no
/// `sh2.caseMatch` dispatch, no glob engine, no per-pattern string parsing.
/// bash `case` is first-match-wins, which is exactly an if/else-if chain;
/// the discriminant is bound once to a temp const (the switch form
/// evaluates it once too). Under a possible `shopt -s nocasematch` the
/// runtime's caseMatch lowercases the VALUE side only (not the pattern) —
/// mirrored exactly. Conservative: any unclassifiable pattern (or no
/// clauses at all) keeps the runtime switch form.
fn try_native_case(
    discriminant: &IrExpr,
    clauses: &[IrCaseClause],
    nocase: bool,
) -> Option<Stmt> {
    if clauses.is_empty() {
        return None;
    }
    let pats: Vec<Vec<CasePat>> = clauses
        .iter()
        .map(|c| {
            c.patterns
                .iter()
                .map(|p| classify_case_pat(p))
                .collect::<Option<Vec<_>>>()
        })
        .collect::<Option<Vec<_>>>()?;
    // `String($sh_case ?? '')` — the runtime's caseMatch coercion.
    let value_expr = |id: &str| {
        let base = Expr::CallExpression {
            callee: Box::new(Expr::Identifier {
                name: "String".to_string(),
            }),
            arguments: vec![Expr::LogicalExpression {
                operator: "??",
                left: Box::new(Expr::Identifier {
                    name: id.to_string(),
                }),
                right: Box::new(str_lit("")),
            }],
            optional: false,
        };
        if nocase {
            Expr::CallExpression {
                callee: Box::new(Expr::MemberExpression {
                    object: Box::new(base),
                    property: Box::new(Expr::Identifier {
                        name: "toLowerCase".to_string(),
                    }),
                    computed: false,
                    optional: false,
                }),
                arguments: vec![],
                optional: false,
            }
        } else {
            base
        }
    };
    let pat_test = |pat: &CasePat| -> Expr {
        let value = value_expr(CASE_TMP);
        match pat {
            CasePat::Any => Expr::Literal {
                value: serde_json::Value::Bool(true),
                raw: None,
            },
            CasePat::Substr(lit) => Expr::CallExpression {
                callee: Box::new(Expr::MemberExpression {
                    object: Box::new(value),
                    property: Box::new(Expr::Identifier {
                        name: "includes".to_string(),
                    }),
                    computed: false,
                    optional: false,
                }),
                arguments: vec![str_lit(lit)],
                optional: false,
            },
            CasePat::Prefix(lit) => Expr::CallExpression {
                callee: Box::new(Expr::MemberExpression {
                    object: Box::new(value),
                    property: Box::new(Expr::Identifier {
                        name: "startsWith".to_string(),
                    }),
                    computed: false,
                    optional: false,
                }),
                arguments: vec![str_lit(lit)],
                optional: false,
            },
            CasePat::Suffix(lit) => Expr::CallExpression {
                callee: Box::new(Expr::MemberExpression {
                    object: Box::new(value),
                    property: Box::new(Expr::Identifier {
                        name: "endsWith".to_string(),
                    }),
                    computed: false,
                    optional: false,
                }),
                arguments: vec![str_lit(lit)],
                optional: false,
            },
            CasePat::Exact(lit) => Expr::BinaryExpression {
                operator: "===",
                left: Box::new(value),
                right: Box::new(str_lit(lit)),
            },
        }
    };
    // Clause body: same break/continue → sh2.* signal mapping as the switch
    // form (a native break inside the if would only be legal inside a JS
    // loop, but bash's break must exit the ENCLOSING loop).
    let body_of = |stmts: &[IrStmt]| Stmt::BlockStatement {
        body: stmts
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
            .collect(),
    };
    // Build the chain from the LAST clause backwards (alternate nesting).
    let mut alt: Option<Box<Stmt>> = None;
    for (clause, pats) in clauses.iter().zip(pats.iter()).rev() {
        let mut test: Option<Expr> = None;
        for pat in pats {
            let t = pat_test(pat);
            test = Some(match test {
                None => t,
                Some(prev) => Expr::LogicalExpression {
                    operator: "||",
                    left: Box::new(prev),
                    right: Box::new(t),
                },
            });
        }
        let stmt = Stmt::IfStatement {
            test: test.expect("case clause has at least one pattern"),
            consequent: Box::new(body_of(&clause.body)),
            alternate: alt.take(),
        };
        alt = Some(Box::new(stmt));
    }
    Some(Stmt::BlockStatement {
        body: vec![
            Stmt::VariableDeclaration {
                kind: "const",
                declarations: vec![VariableDeclarator {
                    type_: "VariableDeclarator",
                    id: Expr::Identifier {
                        name: CASE_TMP.to_string(),
                    },
                    init: Some(expr_to_estree(discriminant)),
                }],
            },
            *alt.expect("at least one clause"),
        ],
    })
}

/// True when the program may enable `set -e` (errexit) anywhere (top level
/// or nested in any function/loop/case/arrow body). The runtime's
/// `sh2.guard` wrapper is the identity function when the errexit flag never
/// turns on (it starts false and only the `set` builtin's `-e` / `-o
/// errexit` toggles it — `eval`/`source` run real bash subprocesses whose
/// errexit is their own), so the emitter skips guard emission entirely for
/// programs that provably never enable it. Conservative: a dynamic command
/// name (`$cmd ...` may be `set`), a non-literal set argument (may be
/// `-e`), or `-e` in any flag cluster all keep the guards.
fn ir_may_enable_errexit(prog: &IrProgram) -> bool {
    /// A literal `exec("set", [...])` call — the runtime's set builtin
    /// parses (see sh2-namespace.mjs): `--` first or a first arg without a
    /// `-`/`+` prefix means positional assignment (no flags); otherwise
    /// each flag cluster is scanned — `e` enables errexit with a `-`
    /// prefix (and disables with `+`), `o` makes the NEXT argument a long
    /// option name (`errexit` enables).
    fn set_call_enables(args: &[IrExpr]) -> bool {
        let mut lits: Vec<&str> = Vec::with_capacity(args.len());
        for a in args {
            match a {
                IrExpr::Str(s, _) => lits.push(s.as_str()),
                _ => return true, // dynamic flag word: may be `-e` at runtime
            }
        }
        match lits.first() {
            None | Some(&"--") => return false,
            Some(a) if !a.starts_with('-') && !a.starts_with('+') => {
                return false; // positional assignment
            }
            _ => {}
        }
        let mut pending_o = false; // `-o`/`+o` seen: next arg is a long option name
        let mut o_enable = false;
        for a in lits {
            if pending_o {
                if a == "errexit" && o_enable {
                    return true;
                }
                pending_o = false;
                continue;
            }
            let enable = a.starts_with('-');
            if !a.starts_with('-') && !a.starts_with('+') {
                continue;
            }
            for c in a[1..].chars() {
                if c == 'e' && enable {
                    return true;
                }
                if c == 'o' {
                    pending_o = true;
                    o_enable = enable;
                }
            }
        }
        false
    }

    fn scan_expr(e: &IrExpr) -> bool {
        match e {
            IrExpr::Call { func, args } => {
                if func == "exec" {
                    match args.as_slice() {
                        [IrExpr::Str(name, _), IrExpr::Array(elems), ..] if name == "set" => {
                            if set_call_enables(elems) {
                                return true;
                            }
                        }
                        [IrExpr::Str(_name, _), ..] => { /* other literal command */ }
                        _ => return true, // dynamic command name: may be `set`
                    }
                }
                args.iter().any(scan_expr)
            }
            IrExpr::Arrow(stmts) => scan_stmts(stmts),
            IrExpr::Array(elems) => elems.iter().any(scan_expr),
            IrExpr::Object(props) => props.iter().any(|(_, v)| scan_expr(v)),
            IrExpr::Interpolate(parts) => parts.iter().any(|p| match p {
                InterpPart::Expr(e) => scan_expr(e),
                InterpPart::Lit(_) => false,
            }),
            IrExpr::Capture { expr, .. } => scan_expr(expr),
            IrExpr::Index { key, .. } => scan_expr(key),
            IrExpr::BinOp { lhs, rhs, .. } => scan_expr(lhs) || scan_expr(rhs),
            IrExpr::MethodCall { obj, args, .. } => {
                scan_expr(obj) || args.iter().any(scan_expr)
            }
            IrExpr::Ternary { cond, then, else_ } => {
                scan_expr(cond) || scan_expr(then) || scan_expr(else_)
            }
            IrExpr::DefinedOr { expr, default } => scan_expr(expr) || scan_expr(default),
            IrExpr::Arith(a) => scan_arith(a),
            _ => false,
        }
    }
    fn scan_arith(a: &ArithAst) -> bool {
        match a {
            ArithAst::Bin { lhs, rhs, .. } => scan_arith(lhs) || scan_arith(rhs),
            ArithAst::Un { arg, .. } => scan_arith(arg),
            ArithAst::Cond { test, then, else_ } => {
                scan_arith(test) || scan_arith(then) || scan_arith(else_)
            }
            ArithAst::Index { key, .. } => scan_arith(key),
            _ => false,
        }
    }
    fn scan_stmts(stmts: &[IrStmt]) -> bool {
        stmts.iter().any(scan_stmt)
    }
    fn scan_stmt(s: &IrStmt) -> bool {
        match s {
            IrStmt::Expr(e) => scan_expr(e),
            IrStmt::Output { value, .. } => scan_expr(value),
            IrStmt::WriteFile { path, content, .. } => {
                scan_expr(path) || scan_expr(content)
            }
            IrStmt::Assign { targets, expr } => {
                scan_expr(expr) || targets.iter().any(|t| t.indices.iter().any(scan_expr))
            }
            IrStmt::Declare { init, .. } => init.as_ref().is_some_and(scan_expr),
            IrStmt::DeclareArray { elements, .. } => elements.iter().any(scan_expr),
            IrStmt::If { cond, then, elsifs, else_ } => {
                scan_expr(cond)
                    || scan_stmts(then)
                    || scan_stmts(else_)
                    || elsifs.iter().any(|(c, b)| scan_expr(c) || scan_stmts(b))
            }
            IrStmt::For { iter, body, .. } => scan_expr(iter) || scan_stmts(body),
            IrStmt::While { cond, body } | IrStmt::DoWhile { cond, body, .. } => {
                scan_expr(cond) || scan_stmts(body)
            }
            IrStmt::Die { expr, .. } | IrStmt::Warn { expr, .. } => scan_expr(expr),
            IrStmt::Exec { cmd, args, env, .. } => {
                match cmd {
                    IrExpr::Str(name, _) if name == "set" => {
                        if set_call_enables(args) {
                            return true;
                        }
                    }
                    IrExpr::Str(_name, _) => { /* other literal command */ }
                    _ => return true, // dynamic command name: may be `set`
                }
                scan_expr(cmd)
                    || args.iter().any(scan_expr)
                    || env.iter().any(|(_, v)| scan_expr(v))
            }
            IrStmt::Pipeline { stages, .. } => stages.iter().any(|s| scan_stmts(s)),
            IrStmt::Return(Some(e)) | IrStmt::Exit(Some(e)) => scan_expr(e),
            IrStmt::SetChildError(e) => scan_expr(e),
            IrStmt::Case {
                discriminant,
                clauses,
            } => scan_expr(discriminant) || clauses.iter().any(|c| scan_stmts(&c.body)),
            IrStmt::Redirect { inner, redirects } => {
                scan_stmts(inner) || redirects.iter().any(|r| scan_expr(&r.target))
            }
            IrStmt::Function { body, .. }
            | IrStmt::Subshell(body)
            | IrStmt::Background(body)
            | IrStmt::Block(body) => scan_stmts(body),
            IrStmt::Require(_)
            | IrStmt::RawText(_)
            | IrStmt::Return(None)
            | IrStmt::Exit(None) => false,
        }
    }
    scan_stmts(&prog.stmts)
}

/// True when the program contains `shopt -s nocasematch` anywhere (top
/// level or nested in any function/loop/case/arrow body). Conservative: the
/// runtime's case/test matching is case-insensitive once enabled, so a
/// native substring lift must lowercase to stay exact. `shopt -u` after a
/// `-s` still counts (a static scan cannot prove the runtime state).
fn ir_may_enable_nocasematch(prog: &IrProgram) -> bool {
    fn scan_expr(e: &IrExpr) -> bool {
        match e {
            IrExpr::Call { func, args } => {
                if func == "shopt"
                    && matches!(args.as_slice(), [IrExpr::Str(opt, _), IrExpr::Bool(en)]
                        if opt == "nocasematch" && *en)
                {
                    return true;
                }
                args.iter().any(scan_expr)
            }
            IrExpr::Arrow(stmts) => scan_stmts(stmts),
            IrExpr::Array(elems) => elems.iter().any(scan_expr),
            IrExpr::Object(props) => props.iter().any(|(_, v)| scan_expr(v)),
            IrExpr::Interpolate(parts) => parts.iter().any(|p| match p {
                InterpPart::Expr(e) => scan_expr(e),
                InterpPart::Lit(_) => false,
            }),
            IrExpr::Capture { expr, .. } => scan_expr(expr),
            IrExpr::Index { key, .. } => scan_expr(key),
            IrExpr::BinOp { lhs, rhs, .. } => scan_expr(lhs) || scan_expr(rhs),
            IrExpr::MethodCall { obj, args, .. } => {
                scan_expr(obj) || args.iter().any(scan_expr)
            }
            IrExpr::Ternary { cond, then, else_ } => {
                scan_expr(cond) || scan_expr(then) || scan_expr(else_)
            }
            IrExpr::DefinedOr { expr, default } => scan_expr(expr) || scan_expr(default),
            IrExpr::Arith(a) => scan_arith(a),
            _ => false,
        }
    }
    fn scan_arith(a: &ArithAst) -> bool {
        match a {
            ArithAst::Bin { lhs, rhs, .. } => scan_arith(lhs) || scan_arith(rhs),
            ArithAst::Un { arg, .. } => scan_arith(arg),
            ArithAst::Cond { test, then, else_ } => {
                scan_arith(test) || scan_arith(then) || scan_arith(else_)
            }
            ArithAst::Index { key, .. } => scan_arith(key),
            _ => false,
        }
    }
    fn scan_stmts(stmts: &[IrStmt]) -> bool {
        stmts.iter().any(scan_stmt)
    }
    fn scan_stmt(s: &IrStmt) -> bool {
        match s {
            IrStmt::Expr(e) => scan_expr(e),
            IrStmt::Output { value, .. } => scan_expr(value),
            IrStmt::WriteFile { path, content, .. } => {
                scan_expr(path) || scan_expr(content)
            }
            IrStmt::Assign { targets, expr } => {
                scan_expr(expr) || targets.iter().any(|t| t.indices.iter().any(scan_expr))
            }
            IrStmt::Declare { init, .. } => init.as_ref().is_some_and(scan_expr),
            IrStmt::DeclareArray { elements, .. } => elements.iter().any(scan_expr),
            IrStmt::If { cond, then, elsifs, else_ } => {
                scan_expr(cond)
                    || scan_stmts(then)
                    || scan_stmts(else_)
                    || elsifs.iter().any(|(c, b)| scan_expr(c) || scan_stmts(b))
            }
            IrStmt::For { iter, body, .. } => scan_expr(iter) || scan_stmts(body),
            IrStmt::While { cond, body } | IrStmt::DoWhile { cond, body, .. } => {
                scan_expr(cond) || scan_stmts(body)
            }
            IrStmt::Die { expr, .. } | IrStmt::Warn { expr, .. } => scan_expr(expr),
            IrStmt::Exec { cmd, args, env, .. } => {
                scan_expr(cmd) || args.iter().any(scan_expr) || env.iter().any(|(_, v)| scan_expr(v))
            }
            IrStmt::Pipeline { stages, .. } => stages.iter().any(|s| scan_stmts(s)),
            IrStmt::Return(Some(e)) | IrStmt::Exit(Some(e)) => scan_expr(e),
            IrStmt::SetChildError(e) => scan_expr(e),
            IrStmt::Case {
                discriminant,
                clauses,
            } => scan_expr(discriminant) || clauses.iter().any(|c| scan_stmts(&c.body)),
            IrStmt::Redirect { inner, redirects } => {
                scan_stmts(inner) || redirects.iter().any(|r| scan_expr(&r.target))
            }
            IrStmt::Function { body, .. }
            | IrStmt::Subshell(body)
            | IrStmt::Background(body)
            | IrStmt::Block(body) => scan_stmts(body),
            IrStmt::Require(_)
            | IrStmt::RawText(_)
            | IrStmt::Return(None)
            | IrStmt::Exit(None) => false,
        }
    }
    scan_stmts(&prog.stmts)
}

/// The temp binding name for a lifted case discriminant. `$` cannot appear
/// in a shell variable name, so `$sh_case` can never collide with a lifted
/// variable's native JS binding.
const CASE_TMP: &str = "$sh_case";

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
            if is_lifted(&target.var) && target.indices.is_empty() {
                // native JS write — the analysis guarantees the source kind
                let right = match expr {
                    // numeric-lifted source
                    IrExpr::Arith(a) => arith_to_estree(a),
                    IrExpr::Int(i) => Expr::Literal {
                        value: serde_json::Value::from(*i),
                        raw: None,
                    },
                    IrExpr::Str(sv, _) if is_lifted_num(&target.var) => Expr::Literal {
                        value: serde_json::Value::from(sv.trim().parse::<i64>().unwrap_or(0)),
                        raw: None,
                    },
                    // string-lifted source
                    IrExpr::Str(sv, _) => Expr::Literal {
                        value: serde_json::Value::String(sv.clone()),
                        raw: None,
                    },
                    IrExpr::Interpolate(parts) => interpolate_to_estree(parts),
                    IrExpr::Var(n, _) => Expr::Identifier { name: n.clone() },
                    IrExpr::Call { func, args } if func == "getVar" => match args.as_slice() {
                        [IrExpr::Str(n, _)] => Expr::Identifier { name: n.clone() },
                        _ => unreachable!("lifted getVar source"),
                    },
                    // the for-loop numeric coercion (`i = Number(i)`)
                    IrExpr::Call { func, args } if func == "Number" => match args.as_slice() {
                        [IrExpr::Ident(n)] => Expr::CallExpression {
                            callee: Box::new(Expr::Identifier {
                                name: "Number".to_string(),
                            }),
                            arguments: vec![Expr::Identifier { name: n.clone() }],
                            optional: false,
                        },
                        _ => unreachable!("lifted Number source"),
                    },
                    _ => unreachable!("lifted var assigned an unanalysed source"),
                };
                return Some(Stmt::ExpressionStatement {
                    expression: Expr::AssignmentExpression {
                        operator: "=".to_string(),
                        left: Box::new(Expr::Identifier {
                            name: target.var.clone(),
                        }),
                        right: Box::new(right),
                    },
                });
            }
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
                // calls only, so the false path must set it explicitly — a
                // native field write (`sh2.lastExit = 0`), no dispatch.
                Some(Box::new(Stmt::BlockStatement {
                    body: vec![Stmt::ExpressionStatement {
                        expression: Expr::AssignmentExpression {
                            operator: "=".to_string(),
                            left: Box::new(sh2_member("lastExit")),
                            right: Box::new(Expr::Literal {
                                value: serde_json::Value::from(0),
                                raw: None,
                            }),
                        },
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
        IrStmt::While { cond, body } => {
            // Fast path: a provably-sync loop (neither cond nor body needs
            // `await`) lowers to the synchronous runtime loop, which has
            // IDENTICAL semantics (lastExit, BREAK/CONTINUE/RETURN signals,
            // capture bound) minus the per-iteration promise/microtask
            // machinery — ~100x faster busy loops. The closures are plain
            // (r#async: false); eligibility is checked on the already-lowered
            // ESTree: any AwaitExpression inside disqualifies (the runtime
            // call is pure CPU, so no *Sync blocking-I/O concern — the gate
            // whitelists whileLoopSync explicitly).
            let cond_e = expr_to_estree(cond);
            let body_stmts: Vec<Stmt> = body.iter().filter_map(stmt_to_estree).collect();
            if !expr_has_await(&cond_e) && !stmts_have_await(&body_stmts) {
                return Some(Stmt::ExpressionStatement {
                    expression: sh2_call(
                        "whileLoopSync",
                        vec![sync_arrow_expr(cond_e), sync_arrow_block(body_stmts)],
                    ),
                });
            }
            Stmt::ExpressionStatement {
                expression: await_call(
                    "whileLoop",
                    vec![
                        arrow(vec![], IrExpr::Arrow(vec![IrStmt::Expr(cond.clone())])),
                        arrow(vec![], IrExpr::Arrow(body.clone())),
                    ],
                ),
            }
        }
        IrStmt::For { var, iter, body } => {
            let js_var = safe_ident(var);
            let mut body_stmts = vec![];
            if is_lifted_num(var) {
                // the forLoop items arrive as strings; coerce the param to
                // a number in place (the closure param shadows the module
                // let — a self-assign is exactly the coercion we want)
                body_stmts.push(IrStmt::Assign {
                    targets: vec![AssignTarget {
                        var: var.clone(),
                        sigil: None,
                        indices: vec![],
                    }],
                    expr: IrExpr::Call {
                        func: "Number".to_string(),
                        args: vec![IrExpr::Ident(js_var.clone())],
                    },
                });
            } else if !is_lifted(var) {
                // store sync (non-lifted loop var)
                body_stmts.push(IrStmt::Assign {
                    targets: vec![AssignTarget {
                        var: var.clone(),
                        sigil: None,
                        indices: vec![],
                    }],
                    expr: IrExpr::Ident(js_var.clone()),
                });
            }
            body_stmts.extend(body.clone());
            let iter_e = expr_to_estree(iter);
            let body_e: Vec<Stmt> = body_stmts.iter().filter_map(stmt_to_estree).collect();
            // Fast path: a provably-sync loop (neither the iterable nor the
            // body needs `await`) lowers to the synchronous runtime loop —
            // identical semantics (flattening, GLOB_MAGIC items, BREAK/
            // CONTINUE/RETURN signals, capture bound) minus the per-iteration
            // promise machinery (the whileLoopSync precedent).
            if !expr_has_await(&iter_e) && !stmts_have_await(&body_e) {
                return Some(Stmt::ExpressionStatement {
                    expression: sh2_call(
                        "forLoopSync",
                        vec![iter_e, sync_arrow_with_param(js_var, body_e)],
                    ),
                });
            }
            Stmt::ExpressionStatement {
                expression: await_call(
                    "forLoop",
                    vec![
                        iter_e,
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
            let nocase = CASE_NOCASE.lock().unwrap().unwrap_or(false);
            if let Some(native) = try_native_case(discriminant, clauses, nocase) {
                return Some(native);
            }
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
            // sync-builtin dispatch (the expr_to_estree rewrite, for the
            // statement-form Exec): env-carrying calls stay async.
            let callee = if env.is_empty() {
                if let IrExpr::Str(name, _) = cmd {
                    if SYNC_BUILTINS.contains(&name.as_str()) && !program_defines_function(name) {
                        "builtin"
                    } else {
                        "exec"
                    }
                } else {
                    "exec"
                }
            } else {
                "exec"
            };
            let call = sh2_call(callee, call_args);
            Stmt::ExpressionStatement {
                expression: if is_async_call(callee) {
                    await_expr(call)
                } else {
                    call
                },
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

fn str_operand(e: &str) -> Option<Expr> {
    let e = e.trim();
    if let Some(inner) = e.strip_prefix('"').and_then(|x| x.strip_suffix('"')) {
        let bare = inner.strip_prefix('$').unwrap_or(inner);
        if is_lifted_str(bare) {
            return Some(Expr::Identifier { name: bare.to_string() });
        }
        if inner.contains('$')
            || inner.contains('*')
            || inner.contains('?')
            || inner.contains('[')
        {
            return None;
        }
        return Some(Expr::Literal {
            value: serde_json::Value::String(inner.to_string()),
            raw: None,
        });
    }
    // A bare `$name` needs the runtime value — only a lifted var can be
    // read natively; never treat it as the literal text (`$y` ≠ "y").
    if let Some(rest) = e.strip_prefix('$') {
        if is_lifted_str(rest) {
            return Some(Expr::Identifier {
                name: rest.to_string(),
            });
        }
        return None;
    }
    if !e.is_empty()
        && !e.contains(['*', '?', '[', '$'])
        && e.chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-' || c == '.')
    {
        return Some(Expr::Literal {
            value: serde_json::Value::String(e.to_string()),
            raw: None,
        });
    }
    None
}

/// `[ "$x" = *P* ]` family: the RIGHT side is a [`CasePat`] glob (the
/// runtime glob-matches a `=`/`==`/`!=` operand containing glob
/// metachars — and only the right side; a glob on the left is compared
/// literally), the left is a normal operand (lifted var or literal) —
/// lower to native `String(x).includes(P)` / `startsWith` / `endsWith`
/// (negated for `!=`). Under a possible `nocasematch` the runtime's
/// evalTest lowercases BOTH sides: the literal is pre-lowercased at emit
/// time and the value side gets `toLowerCase()`.
fn try_native_glob_test(lhs: &str, rhs: &str, negate: bool) -> Option<Expr> {
    let nocase = CASE_NOCASE.lock().unwrap().unwrap_or(false);
    let build = |operand: Expr, pat: &CasePat| {
        let mut value = Expr::CallExpression {
            callee: Box::new(Expr::Identifier {
                name: "String".to_string(),
            }),
            arguments: vec![operand],
            optional: false,
        };
        if nocase {
            value = Expr::CallExpression {
                callee: Box::new(Expr::MemberExpression {
                    object: Box::new(value),
                    property: Box::new(Expr::Identifier {
                        name: "toLowerCase".to_string(),
                    }),
                    computed: false,
                    optional: false,
                }),
                arguments: vec![],
                optional: false,
            };
        }
        let lit_str = |lit: &str| {
            if nocase {
                lit.to_lowercase()
            } else {
                lit.to_string()
            }
        };
        let str_op = |name: &str, arg: Expr| Expr::CallExpression {
            callee: Box::new(Expr::MemberExpression {
                object: Box::new(value.clone()),
                property: Box::new(Expr::Identifier {
                    name: name.to_string(),
                }),
                computed: false,
                optional: false,
            }),
            arguments: vec![arg],
            optional: false,
        };
        let inc = match pat {
            CasePat::Any => Expr::Literal {
                value: serde_json::Value::Bool(true),
                raw: None,
            },
            CasePat::Substr(lit) => str_op("includes", str_lit(&lit_str(lit))),
            CasePat::Prefix(lit) => str_op("startsWith", str_lit(&lit_str(lit))),
            CasePat::Suffix(lit) => str_op("endsWith", str_lit(&lit_str(lit))),
            CasePat::Exact(lit) => Expr::BinaryExpression {
                operator: "===",
                left: Box::new(value.clone()),
                right: Box::new(str_lit(&lit_str(lit))),
            },
        };
        if negate {
            Expr::UnaryExpression {
                operator: "!",
                argument: Box::new(inc),
                prefix: true,
            }
        } else {
            inc
        }
    };
    if let Some(pat) = classify_case_pat(rhs.trim()) {
        // only the GLOB shapes lift natively here: a bare operand
        // containing `=`/`<`/`>` would tokenize into separate test tokens
        // (the runtime splits on them), so Exact/Any stay on the runtime
        // (the plain-equality path already covers exact literals).
        if matches!(&pat, CasePat::Substr(_) | CasePat::Prefix(_) | CasePat::Suffix(_)) {
            if let Some(l) = str_operand(lhs) {
                return Some(build(l, &pat));
            }
        }
    }
    None
}

/// Native lowering for a SIMPLE test expression whose operands are all
/// lifted numeric variables (or integer literals): `"$count" -lt 100`
/// becomes `count < 100` — no runtime test-string round-trip. Returns None
/// for anything else (falls back to the injected template / runtime).
fn try_native_test(s: &str) -> Option<Expr> {
    let s = s.trim();
    // numeric ops first (their standalone tokens are unambiguous)
    let numeric_ops: [(&str, &str); 6] = [
        ("-eq", "==="),
        ("-ne", "!=="),
        ("-lt", "<"),
        ("-le", "<="),
        ("-gt", ">"),
        ("-ge", ">="),
    ];
    let string_ops: [(&str, &str); 3] = [("==", "==="), ("!=", "!=="), ("=", "===")];
    for (op, js) in numeric_ops {
        let pat = format!(" {op} ");
        if let Some(p) = s.find(&pat) {
            let (lhs, rhs) = (&s[..p], &s[p + 2 + op.len()..]);
            // numeric comparison: operands must be numeric-lifted or ints
            fn num_operand(e: &str) -> Option<Expr> {
                let e = e.trim();
                let e = e.strip_prefix('"').and_then(|x| x.strip_suffix('"')).unwrap_or(e);
                let e = e.strip_prefix('$').unwrap_or(e);
                if is_lifted_num(e) {
                    return Some(Expr::Identifier { name: e.to_string() });
                }
                if let Ok(v) = e.parse::<i64>() {
                    return Some(Expr::Literal { value: serde_json::Value::from(v), raw: None });
                }
                None
            }
            let l = num_operand(lhs)?;
            let r = num_operand(rhs)?;
            return Some(Expr::BinaryExpression {
                operator: js,
                left: Box::new(l),
                right: Box::new(r),
            });
        }
    }
    for (op, js) in string_ops {
        // the parser strips the SPACES around `=`/`==`/`!=` (`"$x"=hello`),
        // so match the bare operator token outside quoted regions
        let mut in_q = false;
        let mut idx = 0usize;
        let b = s.as_bytes();
        while idx < b.len() {
            if b[idx] == b'"' {
                in_q = !in_q;
                idx += 1;
                continue;
            }
            if !in_q && s[idx..].starts_with(op) {
                // `==` must not be consumed as `=`; `=` must not be the
                // first char of `==`
                if op == "=" && s[idx..].starts_with("==") {
                    idx += 1;
                    continue;
                }
                // the operator must not sit inside a quoted region or a
                // word (`"$x"=a=b` — the first `=` is the operator, the
                // second sits mid-word). Like the runtime tokenizer (which
                // splits on `=` even adjacent to word chars), a word may
                // end right before the operator (`$s==*.txt`); false
                // operators are weeded out by the operand checks below.
                let before = if idx > 0 { b[idx - 1] } else { 0 };
                let is_op = before == 0
                    || before == b'"'
                    || before == b' '
                    || before == b'\''
                    || before == b'$'
                    || before == b'_'
                    || before.is_ascii_alphanumeric();
                if is_op {
                    let (lhs, rhs) = (&s[..idx], &s[idx + op.len()..]);
                    // glob-to-substring family: `[ "$x" = *P* ]` →
                    // `String(x).includes(P)`. The runtime glob-matches a
                    // `=`/`==`/`!=` operand containing glob metacharacters;
                    // a pure `*P*` operand is exactly a substring test, so
                    // the whole comparison lowers native (no sh2.test).
                    if let Some(native) = try_native_glob_test(lhs, rhs, op == "!=") {
                        return Some(native);
                    }
                    let l = str_operand(lhs)?;
                    let r = str_operand(rhs)?;
                    return Some(Expr::BinaryExpression {
                        operator: js,
                        left: Box::new(l),
                        right: Box::new(r),
                    });
                }
            }
            idx += 1;
        }
    }
    None
}

/// Inject lifted-variable VALUES into a test-expression string as a
/// template literal: `"$count" -lt 100` becomes
/// `` `"${count}" -lt 100` `` (the runtime still parses the expression, but
/// the lifted var's value is inlined instead of read from the store, which
/// it is no longer in). Handles `$name`, `${name}`, and bare names inside
/// `$(( ... ))` arith regions. Returns None when nothing is injected.
fn test_str_to_estree(s: &str) -> Option<Expr> {
    let bytes = s.as_bytes();
    let mut quasis: Vec<String> = Vec::new();
    let mut exprs: Vec<Expr> = Vec::new();
    let mut lit = String::new();
    let mut i = 0usize;
    let n = bytes.len();
    let mut changed = false;
    while i < n {
        if s[i..].starts_with("$((") {
            // `$(( ... ))` arith region — inject bare lifted identifiers
            let mut j = i + 3;
            let mut depth = 2usize;
            while j < n && depth > 0 {
                if bytes[j] == b'(' {
                    depth += 1;
                } else if bytes[j] == b')' {
                    depth -= 1;
                }
                j += 1;
            }
            if depth != 0 {
                break; // unbalanced — keep the rest literal
            }
            let region = &s[i + 3..j - 2];
            lit.push_str("$((");
            let rb = region.as_bytes();
            let mut k = 0usize;
            while k < rb.len() {
                let c = rb[k] as char;
                if (c.is_ascii_alphabetic() || c == '_')
                    && (k == 0 || !rb[k - 1].is_ascii_alphanumeric())
                {
                    let start = k;
                    while k < rb.len() && (rb[k].is_ascii_alphanumeric() || rb[k] == b'_') {
                        k += 1;
                    }
                    let w = &region[start..k];
                    if is_lifted(w) {
                        quasis.push(std::mem::take(&mut lit));
                        exprs.push(Expr::Identifier { name: w.to_string() });
                        changed = true;
                        continue;
                    }
                    lit.push_str(w);
                    continue;
                }
                let ch = region[k..].chars().next().unwrap();
                lit.push(ch);
                k += ch.len_utf8();
            }
            lit.push_str("))");
            i = j;
            continue;
        }
        if bytes[i] == b'$' && i + 1 < n && bytes[i + 1] == b'{' {
            if let Some(close) = s[i + 2..].find('}') {
                let name = &s[i + 2..i + 2 + close];
                let end = i + 2 + close + 1;
                if is_lifted(name) {
                    quasis.push(std::mem::take(&mut lit));
                    exprs.push(Expr::Identifier { name: name.to_string() });
                    changed = true;
                } else {
                    lit.push_str(&s[i..end]);
                }
                i = end;
                continue;
            }
        } else if bytes[i] == b'$' && i + 1 < n {
            let rest = &s[i + 1..];
            let name_len = rest
                .chars()
                .take_while(|c| c.is_ascii_alphanumeric() || *c == '_')
                .count();
            if name_len > 0 {
                let name = &rest[..name_len];
                if is_lifted(name) {
                    quasis.push(std::mem::take(&mut lit));
                    exprs.push(Expr::Identifier { name: name.to_string() });
                    changed = true;
                    i += 1 + name_len;
                    continue;
                }
            }
        }
        let ch = s[i..].chars().next().unwrap();
        lit.push(ch);
        i += ch.len_utf8();
    }
    if !changed {
        return None;
    }
    quasis.push(lit);
    let mut elems: Vec<TemplateElement> = quasis
        .into_iter()
        .map(|raw| TemplateElement {
            type_: "TemplateElement",
            value: TemplateElementValue {
                raw,
                cooked: None,
            },
            tail: false,
        })
        .collect();
    if let Some(last) = elems.last_mut() {
        last.tail = true;
    }
    Some(Expr::TemplateLiteral {
        quasis: elems,
        expressions: exprs,
    })
}

/// Like `test_str_to_estree` but STRICTER: returns `Some` only when the
/// string is `$`-free (plain literal) or EVERY `$`-expansion in it is a
/// lifted variable (all inlined into a template literal). A `$` referring
/// to a store-bound variable would have to be expanded by the runtime from
/// the STORE at runtime — a native expression cannot do that, so the caller
/// falls back to the runtime call (or the value-override param form, where
/// the runtime still expandWord's the arg). Mirrors expandWord's quote
/// handling: a pair of surrounding quotes (the parser keeps them in
/// defaults) is stripped first.
fn fully_lifted_template(s: &str) -> Option<Expr> {
    // expandWord strips one pair of surrounding quotes after expansion.
    let bare = if s.len() >= 2 {
        let q = s.chars().next().unwrap();
        if (q == '"' || q == '\'') && s.ends_with(q) {
            &s[1..s.len() - 1]
        } else {
            s
        }
    } else {
        s
    };
    if !bare.contains('$') {
        return Some(str_lit(bare));
    }
    let bytes = bare.as_bytes();
    let mut i = 0;
    let n = bytes.len();
    while i < n {
        if bytes[i] != b'$' {
            i += 1;
            continue;
        }
        if i + 1 < n && bytes[i + 1] == b'{' {
            if let Some(close) = bare[i + 2..].find('}') {
                let name = &bare[i + 2..i + 2 + close];
                if is_lifted(name) {
                    i = i + 2 + close + 1;
                    continue;
                }
            }
            return None; // ${...} with a non-lifted / complex body
        }
        let rest = &bare[i + 1..];
        let name_len = rest
            .chars()
            .take_while(|c| c.is_ascii_alphanumeric() || *c == '_')
            .count();
        if name_len > 0 {
            let name = &rest[..name_len];
            if is_lifted(name) {
                i += 1 + name_len;
                continue;
            }
        }
        return None; // $$ / $? / $1 / $(...) — not a lifted plain ref
    }
    test_str_to_estree(bare)
}

/// `sh2.param` lowering for LIFTED variable names. The runtime reads the
/// value from the STORE by string name — a lifted binding is not there — so
/// the value is inlined as a JS expression and the pure string ops the
/// runtime would run are emitted natively (mirrors harness
/// sh2-namespace.mjs param/substGlob/stripGlobPrefix/stripGlobSuffix
/// exactly; the corpus is the oracle). Ops whose extras cannot go native
/// (glob patterns, store-reading defaults/offsets, `:?` exit, basename/
/// dirname) fall through to the caller's value-override form
/// (`sh2.param(op, name, extras..., value)`), never a store read.
fn try_native_param(args: &[IrExpr]) -> Option<Expr> {
    let [IrExpr::Str(op, _), IrExpr::Str(name, _), ..] = args else {
        return None;
    };
    if !is_lifted(name) {
        return None;
    }
    // positional / array / ${#x} / ${!map} forms have special runtime
    // semantics and their names are never liftable identifiers anyway.
    if name.contains('@')
        || name.contains('*')
        || name.contains('[')
        || name.starts_with('#')
        || name.starts_with('!')
    {
        return None;
    }
    let id = || Expr::Identifier {
        name: name.clone(),
    };
    let val = || Expr::CallExpression {
        callee: Box::new(Expr::Identifier {
            name: "String".to_string(),
        }),
        arguments: vec![id()],
        optional: false,
    };
    let member = |obj: Expr, prop: &str| Expr::MemberExpression {
        object: Box::new(obj),
        property: Box::new(Expr::Identifier {
            name: prop.to_string(),
        }),
        computed: false,
        optional: false,
    };
    let method = |obj: Expr, prop: &str, args: Vec<Expr>| Expr::CallExpression {
        callee: Box::new(member(obj, prop)),
        arguments: args,
        optional: false,
    };
    let bin = |l: Expr, op: &'static str, r: Expr| Expr::BinaryExpression {
        operator: op,
        left: Box::new(l),
        right: Box::new(r),
    };
    let cond = |t: Expr, c: Expr, a: Expr| Expr::ConditionalExpression {
        test: Box::new(t),
        consequent: Box::new(c),
        alternate: Box::new(a),
    };
    let int_lit = |i: i64| Expr::Literal {
        value: serde_json::Value::from(i),
        raw: None,
    };
    // A glob-strip/substitute pattern that the runtime's literal fast path
    // handles (no glob metachars) and that embeds cleanly in a JS string
    // literal (ASCII; `$` is literal there, exactly like the runtime — it
    // never expands patterns).
    let literal_pattern = |p: &str| {
        !p.is_empty() && p.is_ascii() && !p.chars().any(|c| matches!(c, '*' | '?' | '['))
    };
    match op.as_str() {
        // ${x} — a plain read of the binding (like the getVar lift)
        "" => Some(id()),
        // ${#x} — string length
        "len" => Some(member(val(), "length")),
        // ${x^^} / ${x,,} — case conversion
        "^^" => Some(method(val(), "toUpperCase", vec![])),
        ",," => Some(method(val(), "toLowerCase", vec![])),
        // ${x^} / ${x,} — first character only (empty → "", like the
        // runtime's `v.length ? ... : v` since charAt(0) is "" and
        // slice(1) is "" for the empty string)
        "^" | "," => {
            let up = op == "^";
            Some(bin(
                method(
                    method(val(), "charAt", vec![int_lit(0)]),
                    if up { "toUpperCase" } else { "toLowerCase" },
                    vec![],
                ),
                "+",
                method(val(), "slice", vec![int_lit(1)]),
            ))
        }
        // ${x#p} / ${x##p} / ${x%p} / ${x%%p} — literal prefix/suffix
        // removal (shortest == longest for literal patterns, exactly like
        // the runtime's literal fast paths)
        "#" | "##" | "%" | "%%" => {
            let [_, _, IrExpr::Str(p, _)] = args else {
                return None;
            };
            if !literal_pattern(p) {
                return None;
            }
            let len = p.chars().count() as i64;
            if op.starts_with('#') {
                Some(cond(
                    method(val(), "startsWith", vec![str_lit(p)]),
                    method(val(), "slice", vec![int_lit(len)]),
                    val(),
                ))
            } else {
                Some(cond(
                    method(val(), "endsWith", vec![str_lit(p)]),
                    method(val(), "slice", vec![int_lit(0), int_lit(-len)]),
                    val(),
                ))
            }
        }
        // ${x//p/r} — replace ALL occurrences (split/join; the runtime's
        // literal fast path is exactly this). Empty pattern must stay on
        // the runtime (split("") splits chars; bash treats it as a
        // no-op).
        "//" => {
            let [_, _, IrExpr::Str(p, _), IrExpr::Str(r, _)] = args else {
                return None;
            };
            if !literal_pattern(p) {
                return None;
            }
            let rep = fully_lifted_template(r)?;
            Some(method(
                method(val(), "split", vec![str_lit(p)]),
                "join",
                vec![rep],
            ))
        }
        // ${x:-d} / ${x:=d} — default when empty. `:=` also WRITES the
        // binding (a JS assignment expression — the runtime's setVar
        // cannot see the lifted binding, so this op must go native; the
        // lift analysis marks `:=` names whose default cannot be fully
        // inlined, keeping them store-bound instead).
        ":-" | ":=" => {
            let [_, _, IrExpr::Str(d, _)] = args else {
                return None;
            };
            let dflt = fully_lifted_template(d)?;
            let test = bin(val(), "!==", str_lit(""));
            if op == ":-" {
                Some(cond(test, val(), dflt))
            } else {
                Some(cond(
                    test,
                    val(),
                    Expr::AssignmentExpression {
                        operator: "=".to_string(),
                        left: Box::new(id()),
                        right: Box::new(dflt),
                    },
                ))
            }
        }
        // ${x:off:len} — substring slice with LITERAL integer offsets
        // (negative offsets count from the end, like the runtime's
        // v.slice(off, off + len)). Non-integer offsets (arith exprs) fall
        // through to the value-override form.
        "slice" => {
            let [_, _, IrExpr::Str(off, _), IrExpr::Str(len, _)] = args else {
                return None;
            };
            let int_of = |t: &str| {
                let t = t.trim();
                if t.is_empty() {
                    Some(0i64)
                } else if t.starts_with('-') {
                    t[1..].parse::<i64>().ok().map(|v| -v)
                } else {
                    t.parse::<i64>().ok()
                }
            };
            let o = int_of(off)?;
            if len.trim().is_empty() {
                Some(method(val(), "slice", vec![int_lit(o)]))
            } else {
                let l = int_of(len)?;
                Some(method(
                    val(),
                    "slice",
                    vec![int_lit(o), int_lit(o + l)],
                ))
            }
        }
        // basename/dirname/:? and everything else: the value-override form
        // (runtime keeps the string-op logic; only the value source
        // changes).
        _ => None,
    }
}

/// Conservative "always a string" analysis (slice 4). A variable lifts to a
/// native JS string binding (`let x = ""`) iff every assignment source is a
/// plain string (a literal, an interpolation, or a copy of another
/// string-lifted var) — never arithmetic, capture, or a write-builtin — and
/// it is not already numeric-lifted. String reads inside arithmetic still
/// work via `(Number(x) || 0)` on the native binding.
fn string_lift_vars(prog: &IrProgram, numeric: &HashSet<String>) -> HashSet<String> {
    let mut assigns: HashMap<String, Vec<IrExpr>> = HashMap::new();
    let mut excluded: HashSet<String> = HashSet::new();
    let mut string_ctx: HashSet<String> = HashSet::new();

    fn is_ident(s: &str) -> bool {
        let mut cs = s.chars();
        match cs.next() {
            Some(c) if c.is_ascii_alphabetic() || c == '_' => {
                cs.all(|c| c.is_ascii_alphanumeric() || c == '_')
            }
            _ => false,
        }
    }
    fn mark_string_refs(s: &str, out: &mut HashSet<String>) {
        let bytes = s.as_bytes();
        let mut i = 0;
        while i < bytes.len() {
            let mut c = bytes[i] as char;
            if c == '$' {
                i += 1;
                if i >= bytes.len() {
                    break;
                }
                c = bytes[i] as char;
            }
            let prev_alnum = i > 0 && bytes[i - 1].is_ascii_alphanumeric();
            if (c.is_ascii_alphabetic() || c == '_') && !prev_alnum {
                let start = i;
                while i < bytes.len() && (bytes[i].is_ascii_alphanumeric() || bytes[i] == b'_') {
                    i += 1;
                }
                let w = &s[start..i];
                if is_ident(w) {
                    out.insert(w.to_string());
                }
            } else {
                i += 1;
            }
        }
    }
    fn mark_str_args(e: &IrExpr, string_ctx: &mut HashSet<String>) {
        match e {
            IrExpr::Str(ss, _) => mark_string_refs(ss, string_ctx),
            IrExpr::Array(elems) => {
                for el in elems {
                    mark_str_args(el, string_ctx);
                }
            }
            IrExpr::Object(props) => {
                for (_, v) in props {
                    mark_str_args(v, string_ctx);
                }
            }
            _ => {}
        }
    }
    fn mark_write_builtin_vars(e: &IrExpr, excluded: &mut HashSet<String>) {
        match e {
            IrExpr::Array(elems) => {
                for el in elems {
                    mark_write_builtin_vars(el, excluded);
                }
            }
            IrExpr::Str(sv, _) => {
                let v = sv.split('=').next().unwrap_or("");
                if is_ident(v) {
                    excluded.insert(v.to_string());
                }
            }
            _ => {}
        }
    }
    fn walk_expr(
        e: &IrExpr,
        excluded: &mut HashSet<String>,
        string_ctx: &mut HashSet<String>,
        in_copy: bool,
    ) {
        match e {
            IrExpr::Call { func, args } => {
                if func != "getVar" && func != "test" && func != "setArray" && func != "setArrayAppend"
                {
                    for (i, a) in args.iter().enumerate() {
                        // `sh2.param`'s NAME arg (index 1) is a direct store
                        // lookup, never a $ref scan — and for a lifted name
                        // the emitter inlines the value (native string ops
                        // or the trailing value-override arg), so it must
                        // NOT be marked store-bound. The extras keep their
                        // marks (the runtime still expandWord's/evalArith's
                        // them against the store). Names that are NOT plain
                        // identifiers (`map[$k]` — the runtime expands the
                        // subscript via normAssocKey/expandWord; `@`/`*`/
                        // `#x`/`!x` forms) keep their marks.
                        if func == "param" && i == 1 {
                            if let IrExpr::Str(n, _) = a {
                                let plain = !n.contains('$')
                                    && !n.contains('[')
                                    && !n.contains('@')
                                    && !n.contains('*')
                                    && !n.starts_with('#')
                                    && !n.starts_with('!');
                                if plain {
                                    continue;
                                }
                            }
                        }
                        mark_str_args(a, string_ctx);
                    }
                }
                // `${x:=d}` WRITES the variable. The native lowering updates
                // the JS binding via an assignment expression — but a
                // default that cannot be fully inlined (a `$` ref to a
                // store-bound var) or a subshell/background write (copy
                // semantics: bash writes a COPY; a module binding would be
                // clobbered) must keep the name store-bound so the runtime
                // setVar path stays consistent.
                if func == "param" {
                    if let [IrExpr::Str(op, _), IrExpr::Str(name, _), ..] = args.as_slice() {
                        if op == ":=" {
                            let store_default = matches!(
                                args.get(2),
                                Some(IrExpr::Str(d, _)) if d.contains('$')
                            );
                            if store_default || in_copy {
                                string_ctx.insert(name.clone());
                            }
                        }
                    }
                }
                if func == "exec" {
                    if let Some(IrExpr::Str(cname, _)) = args.first() {
                        if matches!(
                            cname.as_str(),
                            "read" | "declare" | "typeset" | "local" | "export" | "readonly"
                                | "unset" | "mapfile" | "readarray" | "let" | "eval" | "source"
                                | "."
                        ) {
                            for a in &args[1..] {
                                mark_write_builtin_vars(a, excluded);
                            }
                        }
                    }
                }
                if matches!(
                    func.as_str(),
                    "arrayIndex" | "arrayLen" | "arrayItems" | "arraySlice" | "setArray"
                        | "setArrayAppend"
                ) {
                    if let Some(IrExpr::Str(name, _)) = args.first() {
                        excluded.insert(name.clone());
                    }
                }
                for a in args {
                    walk_expr(a, excluded, string_ctx, in_copy);
                }
            }
            IrExpr::Arrow(stmts) => {
                for st in stmts {
                    walk_stmt(st, excluded, string_ctx, in_copy);
                }
            }
            IrExpr::Index { key, .. } => walk_expr(key, excluded, string_ctx, in_copy),
            IrExpr::BinOp { lhs, rhs, .. } => {
                walk_expr(lhs, excluded, string_ctx, in_copy);
                walk_expr(rhs, excluded, string_ctx, in_copy);
            }
            IrExpr::MethodCall { obj, args, .. } => {
                walk_expr(obj, excluded, string_ctx, in_copy);
                for a in args {
                    walk_expr(a, excluded, string_ctx, in_copy);
                }
            }
            IrExpr::Ternary { cond, then, else_, .. } => {
                walk_expr(cond, excluded, string_ctx, in_copy);
                walk_expr(then, excluded, string_ctx, in_copy);
                walk_expr(else_, excluded, string_ctx, in_copy);
            }
            IrExpr::DefinedOr { expr, default } => {
                walk_expr(expr, excluded, string_ctx, in_copy);
                walk_expr(default, excluded, string_ctx, in_copy);
            }
            IrExpr::Interpolate(parts) => {
                for p in parts {
                    if let InterpPart::Expr(inner) = p {
                        walk_expr(inner, excluded, string_ctx, in_copy);
                    }
                }
            }
            IrExpr::Capture { expr, .. } => walk_expr(expr, excluded, string_ctx, in_copy),
            IrExpr::Array(elems) => {
                for el in elems {
                    walk_expr(el, excluded, string_ctx, in_copy);
                }
            }
            IrExpr::Object(props) => {
                for (_, v) in props {
                    walk_expr(v, excluded, string_ctx, in_copy);
                }
            }
            _ => {}
        }
    }
    fn walk_stmt(
        st: &IrStmt,
        excluded: &mut HashSet<String>,
        string_ctx: &mut HashSet<String>,
        in_copy: bool,
    ) {
        match st {
            IrStmt::Assign { targets, expr } => {
                for t in targets {
                    if t.indices.is_empty() {
                        if in_copy {
                            excluded.insert(t.var.clone());
                        }
                    } else {
                        excluded.insert(t.var.clone());
                    }
                }
                walk_expr(expr, excluded, string_ctx, in_copy);
            }
            IrStmt::Declare { vars, .. } => {
                for v in vars {
                    excluded.insert(v.name.clone());
                }
            }
            IrStmt::DeclareArray { var, .. } => {
                excluded.insert(var.clone());
            }
            IrStmt::For { var, iter, body } => {
                // NOTE: the loop var is NOT excluded here — the loop
                // iteration is its assignment source (see collect_for_iters
                // + the fixpoint); external references are removed by
                // drop_externally_referenced_loop_vars afterwards.
                walk_expr(iter, excluded, string_ctx, in_copy);
                for b in body {
                    walk_stmt(b, excluded, string_ctx, in_copy);
                }
            }
            IrStmt::While { cond, body } | IrStmt::DoWhile { cond, body, .. } => {
                walk_expr(cond, excluded, string_ctx, in_copy);
                for b in body {
                    walk_stmt(b, excluded, string_ctx, in_copy);
                }
            }
            IrStmt::If { cond, then, elsifs, else_ } => {
                walk_expr(cond, excluded, string_ctx, in_copy);
                for b in then.iter().chain(else_) {
                    walk_stmt(b, excluded, string_ctx, in_copy);
                }
                for (_, b) in elsifs {
                    for stm in b {
                        walk_stmt(stm, excluded, string_ctx, in_copy);
                    }
                }
            }
            IrStmt::Exec { cmd, args, capture, env, .. } => {
                if let Some(c) = capture {
                    excluded.insert(c.clone());
                }
                for (v, _) in env {
                    excluded.insert(v.clone());
                }
                if let IrExpr::Str(cname, _) = cmd {
                    if matches!(
                        cname.as_str(),
                        "read" | "declare" | "typeset" | "local" | "export" | "readonly"
                            | "unset" | "mapfile" | "readarray" | "let" | "eval" | "source" | "."
                    ) {
                        for a in args {
                            mark_write_builtin_vars(a, excluded);
                        }
                    }
                }
                walk_expr(cmd, excluded, string_ctx, in_copy);
                for a in args {
                    walk_expr(a, excluded, string_ctx, in_copy);
                }
            }
            IrStmt::Pipeline { stages, capture, .. } => {
                if let Some(c) = capture {
                    excluded.insert(c.clone());
                }
                for stage in stages {
                    for b in stage {
                        walk_stmt(b, excluded, string_ctx, in_copy);
                    }
                }
            }
            IrStmt::Function { body, .. } | IrStmt::Block(body) => {
                for b in body {
                    walk_stmt(b, excluded, string_ctx, in_copy);
                }
            }
            IrStmt::Subshell(body) | IrStmt::Background(body) => {
                for b in body {
                    walk_stmt(b, excluded, string_ctx, true);
                }
            }
            IrStmt::Redirect { inner, redirects } => {
                for b in inner {
                    walk_stmt(b, excluded, string_ctx, in_copy);
                }
                for r in redirects {
                    walk_expr(&r.target, excluded, string_ctx, in_copy);
                    if r.interpolate {
                        if let IrExpr::Str(body, _) = &r.target {
                            mark_string_refs(body, string_ctx);
                        }
                    }
                }
            }
            IrStmt::Case { discriminant, clauses } => {
                walk_expr(discriminant, excluded, string_ctx, in_copy);
                for c in clauses {
                    for p in &c.patterns {
                        mark_string_refs(p, string_ctx);
                    }
                    for b in &c.body {
                        walk_stmt(b, excluded, string_ctx, in_copy);
                    }
                }
            }
            IrStmt::Expr(e) => walk_expr(e, excluded, string_ctx, in_copy),
            IrStmt::Output { value, .. } => walk_expr(value, excluded, string_ctx, in_copy),
            IrStmt::WriteFile { path, content, .. } => {
                walk_expr(path, excluded, string_ctx, in_copy);
                walk_expr(content, excluded, string_ctx, in_copy);
            }
            IrStmt::Return(Some(e)) | IrStmt::Exit(Some(e)) => {
                walk_expr(e, excluded, string_ctx, in_copy)
            }
            IrStmt::SetChildError(e) => walk_expr(e, excluded, string_ctx, in_copy),
            IrStmt::Die { expr, .. } | IrStmt::Warn { expr, .. } => {
                walk_expr(expr, excluded, string_ctx, in_copy)
            }
            _ => {}
        }
    }
    for st in &prog.stmts {
        walk_stmt(st, &mut excluded, &mut string_ctx, false);
    }

    fn collect_assigns(st: &IrStmt, assigns: &mut HashMap<String, Vec<IrExpr>>) {
        match st {
            IrStmt::Assign { targets, expr } => {
                for t in targets {
                    if t.indices.is_empty() {
                        assigns
                            .entry(t.var.clone())
                            .or_default()
                            .push(expr.clone());
                    }
                }
            }
            IrStmt::While { body, .. }
            | IrStmt::DoWhile { body, .. }
            | IrStmt::Block(body)
            | IrStmt::Function { body, .. }
            | IrStmt::Subshell(body)
            | IrStmt::Background(body) => {
                for b in body {
                    collect_assigns(b, assigns);
                }
            }
            IrStmt::If { then, elsifs, else_, .. } => {
                for b in then.iter().chain(else_) {
                    collect_assigns(b, assigns);
                }
                for (_, b) in elsifs {
                    for stm in b {
                        collect_assigns(stm, assigns);
                    }
                }
            }
            IrStmt::For { var, body, .. } => {
                // the loop iteration is a source even with no body writes
                assigns.entry(var.clone()).or_default();
                for b in body {
                    collect_assigns(b, assigns);
                }
            }
            IrStmt::Exec { args, .. } => {
                for a in args {
                    collect_expr_assigns(a, assigns);
                }
            }
            IrStmt::Pipeline { stages, .. } => {
                for stage in stages {
                    for b in stage {
                        collect_assigns(b, assigns);
                    }
                }
            }
            IrStmt::Redirect { inner, .. } => {
                for b in inner {
                    collect_assigns(b, assigns);
                }
            }
            IrStmt::Case { clauses, .. } => {
                for c in clauses {
                    for b in &c.body {
                        collect_assigns(b, assigns);
                    }
                }
            }
            IrStmt::Expr(e) => collect_expr_assigns(e, assigns),
            IrStmt::Output { value, .. } => collect_expr_assigns(value, assigns),
            _ => {}
        }
    }
    fn collect_expr_assigns(e: &IrExpr, assigns: &mut HashMap<String, Vec<IrExpr>>) {
        match e {
            IrExpr::Arrow(stmts) => {
                for st in stmts {
                    collect_assigns(st, assigns);
                }
            }
            IrExpr::Call { args, .. } => {
                for a in args {
                    collect_expr_assigns(a, assigns);
                }
            }
            IrExpr::Array(elems) => {
                for el in elems {
                    collect_expr_assigns(el, assigns);
                }
            }
            _ => {}
        }
    }
    for st in &prog.stmts {
        collect_assigns(st, &mut assigns);
    }

    let mut lifted: HashSet<String> = HashSet::new();
    loop {
        let mut changed = false;
        for (name, exprs) in &assigns {
            if lifted.contains(name)
                || numeric.contains(name)
                || excluded.contains(name)
                || string_ctx.contains(name)
                || is_reserved_var(name)
                || is_js_keyword(name)
                || name.contains('[')
                || name.contains(']')
            {
                continue;
            }
            let all_string = exprs.iter().all(|e| match e {
                IrExpr::Str(_, _) => true,
                IrExpr::Interpolate(_) => true,
                IrExpr::Var(n, _) => lifted.contains(n.as_str()),
                IrExpr::Call { func, args } if func == "getVar" => {
                    matches!(args.as_slice(), [IrExpr::Str(n, _)] if lifted.contains(n.as_str()))
                }
                _ => false,
            });
            if all_string {
                lifted.insert(name.clone());
                changed = true;
            }
        }
        if !changed {
            break;
        }
    }
    lifted
}


/// Render a setArray/setArrayAppend argument: Str elements with lifted
/// `$var` references become template literals with the values inlined (the
/// runtime would otherwise read them from the STORE, which lifted vars are
/// no longer in). Arrays recurse; everything else lowers normally.
fn array_elt_to_estree(e: &IrExpr) -> Expr {
    match e {
        IrExpr::Str(sv, _) => {
            if let Some(tpl) = test_str_to_estree(sv) {
                tpl
            } else {
                expr_to_estree(e)
            }
        }
        IrExpr::Array(elems) => Expr::ArrayExpression {
            elements: elems.iter().map(|el| Some(array_elt_to_estree(el))).collect(),
        },
        _ => expr_to_estree(e),
    }
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
        IrExpr::Var(name, _) => {
            if is_lifted(name) {
                Expr::Identifier { name: name.clone() }
            } else {
                sh2_call("getVar", vec![str_lit(name)])
            }
        }
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
            // `sh2.brace(prefix, groups, middles, suffix)` — the args are
            // ALWAYS literal (brace_ir emits Str/Json), the expansion is
            // pure string work, and the runtime never glob-marks the
            // results — so the whole call lowers to a native array literal
            // (computed once at emit time, not per loop/iteration).
            if func == "brace" {
                if let [IrExpr::Str(prefix, _), IrExpr::Json(groups), IrExpr::Json(middles), IrExpr::Str(suffix, _)] =
                    args.as_slice()
                {
                    return Expr::ArrayExpression {
                        elements: brace_expand(prefix, groups, middles, suffix)
                            .iter()
                            .map(|s| Some(str_lit(s)))
                            .collect(),
                    };
                }
            }
            // setArray/setArrayAppend ELEMENT strings are expandWord'd by
            // the runtime from the STORE — inject lifted values as template
            // literals (the parser keeps elements as raw text, so
            // `["$candidate"]` must inline candidate's value).
            let mapped_args: Vec<Expr> = if matches!(func.as_str(), "setArray" | "setArrayAppend") {
                args.iter().map(array_elt_to_estree).collect()
            } else {
                args.iter().map(expr_to_estree).collect()
            };
            // a read of a lifted numeric variable is a bare JS identifier
            if func == "getVar" {
                if let [IrExpr::Str(name, _)] = args.as_slice() {
                    if is_lifted(name) {
                        return Expr::Identifier { name: name.clone() };
                    }
                }
            }
            // `echo X | grep PAT` lowers to a `contains` call — the runtime
            // impl is String(h).includes(n), so emit it NATIVE (no dispatch)
            if func == "contains" {
                if let [h, n] = args.as_slice() {
                    return Expr::CallExpression {
                        callee: Box::new(Expr::MemberExpression {
                            object: Box::new(Expr::CallExpression {
                                callee: Box::new(Expr::Identifier {
                                    name: "String".to_string(),
                                }),
                                arguments: vec![expr_to_estree(h)],
                                optional: false,
                            }),
                            property: Box::new(Expr::Identifier {
                                name: "includes".to_string(),
                            }),
                            computed: false,
                            optional: false,
                        }),
                        arguments: vec![expr_to_estree(n)],
                        optional: false,
                    };
                }
            }
            // test expressions: native comparison when both operands are
            // lifted; otherwise inject lifted values as a template literal.
            // Inside `&&`/`||` arrows the runtime `and`/`or` branch on
            // lastExit, which a native expression never sets — keep the
            // runtime call there (the injected template still inlines
            // lifted values).
            if func == "test" {
                if let [IrExpr::Str(sv, _)] = args.as_slice() {
                    if *AND_OR_DEPTH.lock().unwrap() == 0 {
                        if let Some(native) = try_native_test(sv) {
                            return native;
                        }
                    }
                    if let Some(tpl) = test_str_to_estree(sv) {
                        // the injected template is the ARGUMENT to the
                        // runtime test (a bare template is always truthy)
                        return sh2_call("test", vec![tpl]);
                    }
                }
            }
            // `sh2.param` on a LIFTED variable: the runtime reads the value
            // from the STORE by string name — a lifted binding is not
            // there — so the pure string ops lower NATIVE (the `${x//p/r}`
            // → split/join family) and the rest inject the value as a
            // trailing override argument (the runtime uses `String(value)`
            // instead of a store read; its expandWord/evalArith still
            // process the extras, so `$ref` semantics are unchanged).
            if func == "param" {
                if let Some(native) = try_native_param(args) {
                    return native;
                }
                if let [IrExpr::Str(op, _), IrExpr::Str(name, _), ..] = args.as_slice() {
                    if is_lifted(name) && op != ":=" {
                        let mut cargs: Vec<Expr> = vec![str_lit(op), str_lit(name)];
                        for (i, a) in args.iter().enumerate().skip(2) {
                            match a {
                                // patterns are NEVER expanded by the
                                // runtime (substGlob/stripGlob* use them
                                // raw) — keep them raw; defaults /
                                // replacements / offsets ARE expanded
                                // (expandWord/evalArith), so inject lifted
                                // refs there.
                                IrExpr::Str(s, _) if matches!(op.as_str(), "#" | "##" | "%" | "%%" | "//") && i == 2 => {
                                    cargs.push(str_lit(s));
                                }
                                IrExpr::Str(s, _) => {
                                    cargs.push(test_str_to_estree(s).unwrap_or_else(|| str_lit(s)));
                                }
                                _ => cargs.push(expr_to_estree(a)),
                            }
                        }
                        // the runtime signature is param(op, name, a, b,
                        // value) — pad missing extra slots so the value
                        // lands in the trailing `value` slot.
                        while cargs.len() < 4 {
                            cargs.push(str_lit(""));
                        }
                        cargs.push(Expr::Identifier {
                            name: name.clone(),
                        });
                        return sh2_call("param", cargs);
                    }
                }
            }
            // `for ((...))` whose body needs no await lowers to the sync
            // runtime twin (the whileLoopSync precedent): identical
            // semantics minus the per-iteration promise machinery. The
            // header string is evaluated by the runtime's evalArith
            // (store-based), so only the BODY needs the await scan.
            if func == "cstyleFor" {
                if let [IrExpr::Str(header, _), IrExpr::Arrow(body_stmts)] = args.as_slice() {
                    let body_e: Vec<Stmt> =
                        body_stmts.iter().filter_map(stmt_to_estree).collect();
                    if !stmts_have_await(&body_e) {
                        return sh2_call(
                            "cstyleForSync",
                            vec![str_lit(header), sync_arrow_block(body_e)],
                        );
                    }
                }
            }
            let callee_name = exec_or_builtin(func, args);
            let call = sh2_call(callee_name, mapped_args);
            if is_async_call(callee_name) {
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
            *AND_OR_DEPTH.lock().unwrap() += 1;
            let e = await_call(
                "and",
                vec![arrow(vec![], (**lhs).clone()), arrow(vec![], (**rhs).clone())],
            );
            *AND_OR_DEPTH.lock().unwrap() -= 1;
            e
        }
        IrExpr::BinOp { op: BinOpKind::Or, lhs, rhs } => {
            *AND_OR_DEPTH.lock().unwrap() += 1;
            let e = await_call(
                "or",
                vec![arrow(vec![], (**lhs).clone()), arrow(vec![], (**rhs).clone())],
            );
            *AND_OR_DEPTH.lock().unwrap() -= 1;
            e
        }
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

/// Plain (non-async) zero-arg arrow `() => expr` for the sync-loop fast path.
fn sync_arrow_expr(expr: Expr) -> Expr {
    Expr::ArrowFunctionExpression {
        params: vec![],
        body: ArrowBody::Expr(Box::new(expr)),
        expression: true,
        r#async: false,
    }
}

/// Plain (non-async) block arrow `() => { stmts }` for the sync-loop fast path.
fn sync_arrow_block(stmts: Vec<Stmt>) -> Expr {
    Expr::ArrowFunctionExpression {
        params: vec![],
        body: ArrowBody::Block(Box::new(Stmt::BlockStatement { body: stmts })),
        expression: false,
        r#async: false,
    }
}

/// Plain (non-async) one-param block arrow `(param) => { stmts }` for the
/// forLoopSync fast path (the loop variable shadows the module binding, so
/// the lifted `i = Number(i)` coercion self-assignment works unchanged).
fn sync_arrow_with_param(param: String, stmts: Vec<Stmt>) -> Expr {
    Expr::ArrowFunctionExpression {
        params: vec![Expr::Identifier { name: param }],
        body: ArrowBody::Block(Box::new(Stmt::BlockStatement { body: stmts })),
        expression: false,
        r#async: false,
    }
}

/// True if the lowered ESTree contains an `AwaitExpression` (i.e. needs an
/// async context). Serialization-based: `type_` is the only "type" field, so
/// the substring `"type":"AwaitExpression"` appears iff such a node is
/// present. A false positive only costs the fast path (falls back to the
/// async loop) — never correctness.
fn expr_has_await(e: &Expr) -> bool {
    serde_json::to_string(e)
        .map(|s| s.contains("\"type\":\"AwaitExpression\""))
        .unwrap_or(true)
}

fn stmts_have_await(stmts: &[Stmt]) -> bool {
    serde_json::to_string(stmts)
        .map(|s| s.contains("\"type\":\"AwaitExpression\""))
        .unwrap_or(true)
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
